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

/// Per `31 §4.3`. The struct is `#[non_exhaustive]` so future fields can
/// be added without a breaking change (I10). Construction is via
/// [`Diagnostic::new`], which seeds `severity` from
/// [`Diagnose::severity_default`]; `location` and `notes` are attached
/// via the chainable [`Diagnostic::with_severity`],
/// [`Diagnostic::with_location`], [`Diagnostic::with_note`] mutators.
/// Per-stage consumer crates typically wrap this in their own private
/// `fn diag(kind: K) -> Diagnostic<K>` helper.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct Diagnostic<K: Diagnose> {
    pub kind: K,
    pub severity: Severity,
    pub location: Option<Location>,
    pub notes: Vec<String>,
}

impl<K: Diagnose> Diagnostic<K> {
    /// Construct an envelope around `kind` with `severity` defaulted to
    /// `kind.severity_default()`, no location, and no notes. Per `31 §4.3`.
    pub fn new(kind: K) -> Self {
        let severity = kind.severity_default();
        Self {
            kind,
            severity,
            location: None,
            notes: Vec::new(),
        }
    }

    /// Override the default severity. Per `31 §4.3` (consumer-crate
    /// helpers may upgrade or downgrade severity at construction).
    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// Attach a source [`Location`]. Per `31 §4.3`.
    pub fn with_location(mut self, location: Location) -> Self {
        self.location = Some(location);
        self
    }

    /// Append a free-form note (renders as `note: <text>` after the
    /// primary message). Per `31 §4.3`.
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }
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
    fn diagnostic_new_seeds_severity_from_kind_default() {
        let d = Diagnostic::new(TestKind::Warn);
        assert_eq!(d.severity, Severity::Warning);
        assert!(d.location.is_none());
        assert!(d.notes.is_empty());
    }

    #[test]
    fn diagnostic_with_severity_overrides_default() {
        let d = Diagnostic::new(TestKind::Bad).with_severity(Severity::Warning);
        assert_eq!(d.severity, Severity::Warning);
    }

    #[test]
    fn diagnostic_with_location_attaches_span() {
        let loc = Location {
            source: SourceId::unknown(),
            span: Span { start: 7, end: 11 },
        };
        let d = Diagnostic::new(TestKind::Bad).with_location(loc);
        let span = d.location.expect("location attached").span;
        assert_eq!(span.start, 7);
        assert_eq!(span.end, 11);
    }

    #[test]
    fn diagnostic_with_note_appends_in_order() {
        let d = Diagnostic::new(TestKind::Bad)
            .with_note("first")
            .with_note(String::from("second"));
        assert_eq!(d.notes, vec!["first".to_string(), "second".to_string()]);
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
