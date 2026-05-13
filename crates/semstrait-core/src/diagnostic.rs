//! Typed diagnostic primitives shared across all `semstrait-*` stages.
//!
//! Per `docs/design/apis/30_api_contracts.md` `§5`, every stage emits diagnostics
//! through the generic [`Diagnostic<K>`] envelope. The kind enum `K` is owned
//! by the stage that produces it (e.g. `ParseErrorKind`, `ValidateErrorKind`)
//! and implements [`Diagnose`].
//!
//! Two stage-return shapes are canonical:
//!
//! - **Accumulating** (parse, validate):
//!   `Result<(T, Diagnostics<K>), Diagnostics<K>>` — collects every recoverable
//!   finding before deciding success vs failure.
//! - **Fail-fast** (compile, plan, optimize, adapt):
//!   `Result<T, Diagnostic<K>>` — returns at the first error.
//!
//! Severity::Error in any accumulated diagnostic forces the `Err` arm; warnings
//! ride through on the `Ok` arm with the produced value.

use std::error::Error;
use std::fmt;

/// Severity of a single diagnostic. Error makes the result fail; Warning is
/// informational and travels on the `Ok` arm.
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

/// Logical source label for a diagnostic. Free-form string the caller
/// chooses (e.g. `"model.yaml"`, `"<inline>"`, an HTTP URL).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceId(pub String);

impl SourceId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for SourceId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for SourceId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl fmt::Display for SourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Optional byte-offset span inside a [`SourceId`]. Authors who don't need
/// span detail leave this as `None` on the [`Location`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        debug_assert!(start <= end, "Span start must precede end");
        Self { start, end }
    }
}

/// Pinpoint within a source. Carries a source label and optional span /
/// path (e.g. a YAML pointer like `"/datasets/orders/keys/primary"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Location {
    pub source: SourceId,
    pub span: Option<Span>,
    pub path: Option<String>,
}

impl Location {
    pub fn new(source: impl Into<SourceId>) -> Self {
        Self {
            source: source.into(),
            span: None,
            path: None,
        }
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }
}

/// Stage-owned diagnostic kind. The variant identity IS the diagnostic
/// identity — there is no string `code()` accessor (`30 §5.4`). Callers
/// match on the kind variant directly.
pub trait Diagnose {
    /// Human-facing message text (single line preferred).
    fn message(&self) -> String;

    /// Default severity for this kind. Most variants default to
    /// [`Severity::Error`]; warnings override.
    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    /// Underlying cause, if any (e.g. an `std::io::Error` source).
    fn cause(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

/// One diagnostic. Combines a kind, severity, optional location, and
/// freeform notes that surface alongside the primary message.
#[derive(Debug, Clone)]
pub struct Diagnostic<K: Diagnose> {
    pub kind: K,
    pub severity: Severity,
    pub location: Option<Location>,
    pub notes: Vec<String>,
}

impl<K: Diagnose> Diagnostic<K> {
    /// Construct a diagnostic with the kind's default severity.
    pub fn new(kind: K) -> Self {
        let severity = kind.default_severity();
        Self {
            kind,
            severity,
            location: None,
            notes: Vec::new(),
        }
    }

    /// Construct a diagnostic with an explicit severity (overrides default).
    pub fn with_severity(kind: K, severity: Severity) -> Self {
        Self {
            kind,
            severity,
            location: None,
            notes: Vec::new(),
        }
    }

    pub fn at(mut self, location: Location) -> Self {
        self.location = Some(location);
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }

    pub fn is_warning(&self) -> bool {
        self.severity == Severity::Warning
    }

    /// Map the diagnostic kind to another [`Diagnose`] kind, preserving
    /// severity, location, and notes. Centralises the per-stage "lift
    /// my kind into the fused error type" pattern (`30 §5.6`): callers
    /// pass the destination enum's variant constructor as `f`.
    pub fn map_kind<K2, F>(self, f: F) -> Diagnostic<K2>
    where
        K2: Diagnose,
        F: FnOnce(K) -> K2,
    {
        Diagnostic {
            kind: f(self.kind),
            severity: self.severity,
            location: self.location,
            notes: self.notes,
        }
    }
}

impl<K: Diagnose + fmt::Debug> fmt::Display for Diagnostic<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.severity, self.kind.message())?;
        if let Some(loc) = &self.location {
            write!(f, " (at {})", loc.source)?;
            if let Some(path) = &loc.path {
                write!(f, " {}", path)?;
            }
            if let Some(span) = loc.span {
                write!(f, " [{}..{}]", span.start, span.end)?;
            }
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

/// Vector alias used for both the success-arm warnings and the error-arm
/// accumulator on accumulating stages. Empty vector = no findings.
pub type Diagnostics<K> = Vec<Diagnostic<K>>;

/// Categorize a diagnostic vector by severity. Convenience for stage
/// implementations that need to decide success vs failure based on whether
/// any [`Severity::Error`] entries are present.
pub fn split_by_severity<K: Diagnose>(
    diags: Diagnostics<K>,
) -> (Diagnostics<K>, Diagnostics<K>) {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    for d in diags {
        if d.is_error() {
            errors.push(d);
        } else {
            warnings.push(d);
        }
    }
    (errors, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
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

        fn default_severity(&self) -> Severity {
            match self {
                Self::Bad => Severity::Error,
                Self::Warn => Severity::Warning,
            }
        }
    }

    #[test]
    fn diagnostic_default_severity_from_kind() {
        let d = Diagnostic::new(TestKind::Bad);
        assert!(d.is_error());

        let d = Diagnostic::new(TestKind::Warn);
        assert!(d.is_warning());
    }

    #[test]
    fn split_by_severity_partitions() {
        let diags: Diagnostics<TestKind> = vec![
            Diagnostic::new(TestKind::Bad),
            Diagnostic::new(TestKind::Warn),
            Diagnostic::new(TestKind::Bad),
        ];
        let (errors, warnings) = split_by_severity(diags);
        assert_eq!(errors.len(), 2);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn location_chains_with_path_and_span() {
        let loc = Location::new("model.yaml")
            .with_span(Span::new(10, 20))
            .with_path("/datasets/orders");
        assert_eq!(loc.source.as_str(), "model.yaml");
        assert_eq!(loc.path.as_deref(), Some("/datasets/orders"));
        assert_eq!(loc.span, Some(Span::new(10, 20)));
    }

    #[derive(Debug)]
    enum FusedKind {
        Wrapped(TestKind),
    }

    impl Diagnose for FusedKind {
        fn message(&self) -> String {
            match self {
                Self::Wrapped(k) => k.message(),
            }
        }
        fn default_severity(&self) -> Severity {
            match self {
                Self::Wrapped(k) => k.default_severity(),
            }
        }
    }

    #[test]
    fn map_kind_preserves_envelope_metadata() {
        let d = Diagnostic::new(TestKind::Warn)
            .at(Location::new("x.yaml").with_path("/a"))
            .with_note("hint");

        let fused = d.map_kind(FusedKind::Wrapped);

        assert!(matches!(fused.kind, FusedKind::Wrapped(TestKind::Warn)));
        assert!(fused.is_warning());
        assert_eq!(
            fused.location.as_ref().map(|l| l.source.as_str()),
            Some("x.yaml"),
        );
        assert_eq!(fused.notes, vec!["hint".to_string()]);
    }
}
