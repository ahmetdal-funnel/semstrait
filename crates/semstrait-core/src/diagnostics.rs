//! Unified diagnostics for the semantic-strait compilation pipeline.
//!
//! Every stage (parser, planner, emitter) produces `Diagnostic` values.
//! `CompileError` collects one or more diagnostics and is the public error
//! type returned by the compiler API.

use std::fmt;

// =============================================================================
// Diagnostic
// =============================================================================

/// A single diagnostic message emitted during compilation.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub code: &'static str,
    pub message: String,
    /// Optional structured context (e.g. entity name, field path).
    pub context: Option<DiagnosticContext>,
}

/// Severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticLevel {
    Error,
    Warning,
}

/// Structured context attached to a diagnostic.
#[derive(Debug, Clone)]
pub struct DiagnosticContext {
    /// Dot-delimited path to the offending element (e.g. "kinds.sales.datasets.daily").
    pub path: String,
    /// Name of the entity involved, if applicable.
    pub entity: Option<String>,
}

impl Diagnostic {
    pub fn error(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Error,
            code,
            message: message.into(),
            context: None,
        }
    }

    pub fn warning(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Warning,
            code,
            message: message.into(),
            context: None,
        }
    }

    pub fn with_context(mut self, path: impl Into<String>) -> Self {
        self.context = Some(DiagnosticContext {
            path: path.into(),
            entity: None,
        });
        self
    }

    pub fn with_entity(mut self, path: impl Into<String>, entity: impl Into<String>) -> Self {
        self.context = Some(DiagnosticContext {
            path: path.into(),
            entity: Some(entity.into()),
        });
        self
    }

    pub fn is_error(&self) -> bool {
        self.level == DiagnosticLevel::Error
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let prefix = match self.level {
            DiagnosticLevel::Error => "error",
            DiagnosticLevel::Warning => "warning",
        };
        write!(f, "[{}] {}: {}", self.code, prefix, self.message)?;
        if let Some(ctx) = &self.context {
            write!(f, " (at {})", ctx.path)?;
            if let Some(entity) = &ctx.entity {
                write!(f, " entity={}", entity)?;
            }
        }
        Ok(())
    }
}

// =============================================================================
// CompileError
// =============================================================================

/// Collection of one or more diagnostics that prevented compilation.
///
/// This is the primary error type returned by the public compiler API.
/// It always contains at least one error-level diagnostic.
#[derive(Debug)]
pub struct CompileError {
    diagnostics: Vec<Diagnostic>,
}

impl CompileError {
    /// Create from a single diagnostic.
    pub fn single(diag: Diagnostic) -> Self {
        Self {
            diagnostics: vec![diag],
        }
    }

    /// Create from a vec of diagnostics. Panics if empty (internal invariant).
    pub fn from_diagnostics(diagnostics: Vec<Diagnostic>) -> Self {
        debug_assert!(!diagnostics.is_empty(), "CompileError must have at least one diagnostic");
        Self { diagnostics }
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter().filter(|d| d.is_error())
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter().filter(|d| !d.is_error())
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, d) in self.diagnostics.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{}", d)?;
        }
        Ok(())
    }
}

impl std::error::Error for CompileError {}

// =============================================================================
// ValidationReport — for stages that collect warnings + errors
// =============================================================================

/// Accumulates diagnostics during a validation pass.
///
/// Use `finish()` to either return warnings alongside the result, or
/// fail with a `CompileError` if any errors were collected.
#[derive(Debug, Default)]
pub struct ValidationReport {
    diagnostics: Vec<Diagnostic>,
}

impl ValidationReport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, diag: Diagnostic) {
        self.diagnostics.push(diag);
    }

    pub fn error(&mut self, code: &'static str, message: impl Into<String>) {
        self.push(Diagnostic::error(code, message));
    }

    pub fn warning(&mut self, code: &'static str, message: impl Into<String>) {
        self.push(Diagnostic::warning(code, message));
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.is_error())
    }

    /// Consume the report. Returns `Err` if any errors exist, `Ok(warnings)` otherwise.
    pub fn finish(self) -> Result<Vec<Diagnostic>, CompileError> {
        if self.has_errors() {
            Err(CompileError::from_diagnostics(self.diagnostics))
        } else {
            Ok(self.diagnostics)
        }
    }

    /// Consume the report, discarding warnings. Returns `Err` if any errors exist.
    pub fn finish_discard_warnings(self) -> Result<(), CompileError> {
        if self.has_errors() {
            Err(CompileError::from_diagnostics(self.diagnostics))
        } else {
            Ok(())
        }
    }
}

// =============================================================================
// Error code constants
// =============================================================================

/// Parser stage error codes.
pub mod codes {
    // Parser errors
    pub const PARSE_E001: &str = "PARSE_E001"; // I/O error
    pub const PARSE_E002: &str = "PARSE_E002"; // YAML deserialization
    pub const PARSE_E003: &str = "PARSE_E003"; // structural validation
    pub const PARSE_E004: &str = "PARSE_E004"; // ref resolution
    pub const PARSE_E005: &str = "PARSE_E005"; // nesting violation

    // Constraint errors
    pub const CONST_E001: &str = "CONST_E001"; // dimension one_of violated
    pub const CONST_E002: &str = "CONST_E002"; // dimension none_of violated
    pub const CONST_E003: &str = "CONST_E003"; // dimension all violated
    pub const CONST_E004: &str = "CONST_E004"; // aggregation not allowed
    pub const CONST_E005: &str = "CONST_E005"; // aggregation prohibited
    pub const CONST_E006: &str = "CONST_E006"; // key column aggregation invalid

    // Planner errors
    pub const PLAN_E001: &str = "PLAN_E001"; // no covering dataset
    pub const PLAN_E002: &str = "PLAN_E002"; // joinset anchor not found
    pub const PLAN_E003: &str = "PLAN_E003"; // unreachable dataset in joinset
    pub const PLAN_E004: &str = "PLAN_E004"; // column mapping missing
    pub const PLAN_E005: &str = "PLAN_E005"; // domain mismatch
    pub const PLAN_E006: &str = "PLAN_E006"; // additivity resolution failed

    // Metric errors
    pub const METRC_E001: &str = "METRC_E001"; // chaining depth exceeded

    // User attribute errors
    pub const ATTR_E001: &str = "ATTR_E001"; // missing required user attribute

    // Emit errors
    pub const EMIT_E001: &str = "EMIT_E001"; // unsupported node
    pub const EMIT_E002: &str = "EMIT_E002"; // unsupported expression
    pub const EMIT_E003: &str = "EMIT_E003"; // missing field
    pub const EMIT_E004: &str = "EMIT_E004"; // column not found
    pub const EMIT_E005: &str = "EMIT_E005"; // invalid plan

    // Warnings
    pub const COMP_W001: &str = "COMP_W001"; // partial result (some measures unavailable)
    pub const COMP_W010: &str = "COMP_W010"; // unionset nesting (legal but unusual)
}

// =============================================================================
// From impls for existing error types
// =============================================================================

impl From<crate::parser::ParseError> for CompileError {
    fn from(err: crate::parser::ParseError) -> Self {
        let (code, msg) = match &err {
            crate::parser::ParseError::Io(_) => (codes::PARSE_E001, err.to_string()),
            crate::parser::ParseError::Yaml(_) => (codes::PARSE_E002, err.to_string()),
            crate::parser::ParseError::Validation(_) => (codes::PARSE_E003, err.to_string()),
            crate::parser::ParseError::RefResolution(_) => (codes::PARSE_E004, err.to_string()),
            crate::parser::ParseError::Nesting(_) => (codes::PARSE_E005, err.to_string()),
        };
        CompileError::single(Diagnostic::error(code, msg))
    }
}

impl From<crate::planner::ir::error::EmitError> for CompileError {
    fn from(err: crate::planner::ir::error::EmitError) -> Self {
        let (code, msg) = match &err {
            crate::planner::ir::error::EmitError::UnsupportedNode(_) => {
                (codes::EMIT_E001, err.to_string())
            }
            crate::planner::ir::error::EmitError::UnsupportedExpression(_) => {
                (codes::EMIT_E002, err.to_string())
            }
            crate::planner::ir::error::EmitError::MissingField(_) => {
                (codes::EMIT_E003, err.to_string())
            }
            crate::planner::ir::error::EmitError::ColumnNotFound(_) => {
                (codes::EMIT_E004, err.to_string())
            }
            crate::planner::ir::error::EmitError::InvalidPlan(_) => {
                (codes::EMIT_E005, err.to_string())
            }
        };
        CompileError::single(Diagnostic::error(code, msg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_display() {
        let d = Diagnostic::error(codes::CONST_E001, "one_of constraint violated");
        assert!(d.to_string().contains("CONST_E001"));
        assert!(d.to_string().contains("error"));
    }

    #[test]
    fn test_diagnostic_with_context() {
        let d = Diagnostic::error(codes::PLAN_E001, "no covering dataset")
            .with_entity("kinds.sales", "sales");
        let s = d.to_string();
        assert!(s.contains("kinds.sales"));
        assert!(s.contains("entity=sales"));
    }

    #[test]
    fn test_compile_error_display_multi() {
        let err = CompileError::from_diagnostics(vec![
            Diagnostic::error(codes::CONST_E001, "dim violation"),
            Diagnostic::warning(codes::COMP_W001, "partial result"),
        ]);
        let s = err.to_string();
        assert!(s.contains("CONST_E001"));
        assert!(s.contains("COMP_W001"));
    }

    #[test]
    fn test_validation_report_ok() {
        let mut report = ValidationReport::new();
        report.warning(codes::COMP_W001, "partial");
        let warnings = report.finish().unwrap();
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn test_validation_report_err() {
        let mut report = ValidationReport::new();
        report.warning(codes::COMP_W001, "partial");
        report.error(codes::CONST_E001, "violation");
        let err = report.finish().unwrap_err();
        assert_eq!(err.diagnostics().len(), 2);
        assert_eq!(err.errors().count(), 1);
        assert_eq!(err.warnings().count(), 1);
    }

    #[test]
    fn test_from_parse_error() {
        let pe = crate::parser::ParseError::Validation("bad".to_string());
        let ce: CompileError = pe.into();
        assert!(ce.to_string().contains("PARSE_E003"));
    }
}
