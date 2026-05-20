//! Error types for IR operations.
//!
//! This module hosts both the legacy substrait-conversion errors
//! (`ConvertError`, `SerializeError`, `DeserializeError`) and the narrow
//! ir-emitted error kinds introduced by the second-cascade landing
//! (`STATUS.md` item Q):
//!
//! - [`ValidateError`] — raised by [`crate::tree::Tree::with_new_children`]
//!   and [`crate::tree::Rewriter`] callbacks. Per spec `35 §15.1` /
//!   `14 §3.1`.
//! - [`CompileError`] — raised by `ReturnTypeRule::Custom` callbacks wired
//!   into `FunctionSpec` (when the registry lands in Phase 2b/2c).
//!   Per spec `35 §15.2` / `14a §2`.
//!
//! Both new types drop the legacy `Kind` suffix per the scoped error-naming
//! cleanup tied to the second-cascade landing (`STATUS.md` item Q).
//! Downstream stages embed via D.ii kind-nesting per `30 §7.4` —
//! `model::ValidateError` carries `Ir(ir::ValidateError)`;
//! `manifest::CompileError` carries `Ir(ir::CompileError)`.

use semstrait_core::{Diagnose, Severity};
use thiserror::Error;

/// Error during expression conversion. Legacy type retained for the
/// pre-spec-cascade `substrait` module; new code should use
/// [`ValidateError`] / [`CompileError`].
#[derive(Debug, Error)]
pub enum ConvertError {
    #[error("Column not found in schema: {0}")]
    ColumnNotFound(String),

    #[error("Unsupported expression type: {0}")]
    UnsupportedExpression(String),

    #[error("Invalid expression: {0}")]
    InvalidExpression(String),

    #[error("Type mismatch: {0}")]
    TypeMismatch(String),

    #[error("Function not found: {0}")]
    FunctionNotFound(String),

    #[error("Missing required field: {0}")]
    MissingField(String),
}

/// Error during Substrait serialization. Legacy type retained for the
/// pre-spec-cascade `substrait` module.
#[derive(Debug, Error)]
pub enum SerializeError {
    #[error("Failed to convert expression: {0}")]
    ExpressionConversion(#[from] ConvertError),

    #[error("Invalid plan structure: {0}")]
    InvalidPlan(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Unsupported node type: {0}")]
    UnsupportedNode(String),
}

/// Error during Substrait deserialization. Legacy type retained for the
/// pre-spec-cascade `substrait` module.
#[derive(Debug, Error)]
pub enum DeserializeError {
    #[error("Failed to convert expression: {0}")]
    ExpressionConversion(#[from] ConvertError),

    #[error("Invalid Substrait plan: {0}")]
    InvalidPlan(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Unsupported Substrait construct: {0}")]
    UnsupportedConstruct(String),

    #[error("Schema mismatch: {0}")]
    SchemaMismatch(String),
}

// ── Spec-cascade errors (item Q, 2026-05-19) ───────────────────────────

/// Construction-boundary failure raised by [`crate::tree::Tree::with_new_children`]
/// and [`crate::tree::Rewriter::f_down`] / [`crate::tree::Rewriter::f_up`].
/// Per spec `35 §15.1` / `14 §3.1`.
///
/// The `Kind` suffix is dropped per the scoped error-naming cleanup tied
/// to the second-cascade landing (`STATUS.md` item Q). Downstream stages
/// embed via D.ii kind-nesting per `30 §7.4`.
#[non_exhaustive]
#[derive(Debug, Error, Clone, PartialEq)]
pub enum ValidateError {
    /// `with_new_children` received a child count that does not match the
    /// node's variant tag (e.g. a `BinaryOp` reconstructed with three
    /// children, an `Aggregate` reconstructed with zero args).
    #[error("with_new_children: child count mismatch — expected {expected}, got {got}")]
    ChildCountMismatch { expected: usize, got: usize },

    /// `Aggregate` cannot directly contain another `Aggregate` — the
    /// nesting rule from `14 §3.3`'s structural catalog.
    #[error("Aggregate cannot directly contain another Aggregate")]
    AggregateInAggregate,

    /// `Window` cannot directly contain `Aggregate` or `Window` — the
    /// nesting rule from `14 §3.3`'s structural catalog.
    #[error("Window cannot directly contain Aggregate or Window")]
    InvalidWindowChild,

    /// `Coalesce` requires at least one argument.
    #[error("Coalesce requires at least one argument")]
    EmptyCoalesce,

    /// `InList` requires at least one list element.
    #[error("InList requires at least one list element")]
    EmptyInList,

    /// `Case` requires at least one when-branch.
    #[error("Case requires at least one when-branch")]
    EmptyCase,
}

impl Diagnose for ValidateError {
    fn message(&self) -> String {
        // `thiserror`'s Display impl gives a per-variant message; reuse it.
        format!("{}", self)
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }
}

/// Function return-type computation failure raised by
/// `ReturnTypeRule::Custom` callbacks wired into `FunctionSpec`. Per spec
/// `35 §15.2` / `14a §2`.
///
/// The wider compile-stage error surface (unknown references, ambiguous
/// paths, cycles, type-inference failures) lives in
/// `semstrait-manifest::CompileError` and embeds `Ir(ir::CompileError)`
/// via D.ii kind-nesting per `30 §7.4`.
#[non_exhaustive]
#[derive(Debug, Error, Clone, PartialEq)]
pub enum CompileError {
    /// `ReturnTypeRule::Custom` callback declined to produce a return
    /// type for the supplied argument types.
    #[error("Function `{name}`: return type computation failed — {reason}")]
    ReturnTypeFailure { name: String, reason: String },
}

impl Diagnose for CompileError {
    fn message(&self) -> String {
        format!("{}", self)
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_error_displays_child_count_mismatch() {
        let err = ValidateError::ChildCountMismatch {
            expected: 2,
            got: 3,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("child count mismatch"));
        assert!(msg.contains("expected 2"));
        assert!(msg.contains("got 3"));
    }

    #[test]
    fn validate_error_displays_aggregate_in_aggregate() {
        let err = ValidateError::AggregateInAggregate;
        let msg = format!("{}", err);
        assert!(msg.contains("Aggregate"));
        assert!(msg.contains("directly contain"));
    }

    #[test]
    fn validate_error_displays_invalid_window_child() {
        let err = ValidateError::InvalidWindowChild;
        let msg = format!("{}", err);
        assert!(msg.contains("Window"));
        assert!(msg.contains("Aggregate") || msg.contains("Window"));
    }

    #[test]
    fn validate_error_displays_empty_coalesce() {
        let err = ValidateError::EmptyCoalesce;
        let msg = format!("{}", err);
        assert!(msg.contains("Coalesce"));
        assert!(msg.contains("at least one"));
    }

    #[test]
    fn validate_error_displays_empty_in_list() {
        let err = ValidateError::EmptyInList;
        let msg = format!("{}", err);
        assert!(msg.contains("InList"));
        assert!(msg.contains("at least one"));
    }

    #[test]
    fn validate_error_displays_empty_case() {
        let err = ValidateError::EmptyCase;
        let msg = format!("{}", err);
        assert!(msg.contains("Case"));
        assert!(msg.contains("at least one"));
    }

    #[test]
    fn validate_error_implements_diagnose() {
        let err = ValidateError::EmptyCoalesce;
        // message() must return a non-empty string.
        assert!(!err.message().is_empty());
        // Default severity is Error.
        assert_eq!(err.default_severity(), Severity::Error);
        // No cause chain by default.
        assert!(err.cause().is_none());
    }

    #[test]
    fn validate_error_equality_and_clone() {
        let a = ValidateError::ChildCountMismatch {
            expected: 1,
            got: 2,
        };
        let b = a.clone();
        assert_eq!(a, b);
        let c = ValidateError::EmptyCase;
        assert_ne!(a, c);
    }

    #[test]
    fn compile_error_displays_return_type_failure() {
        let err = CompileError::ReturnTypeFailure {
            name: "coalesce".to_string(),
            reason: "no arguments supplied".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("coalesce"));
        assert!(msg.contains("return type"));
        assert!(msg.contains("no arguments supplied"));
    }

    #[test]
    fn compile_error_implements_diagnose() {
        let err = CompileError::ReturnTypeFailure {
            name: "x".to_string(),
            reason: "y".to_string(),
        };
        assert!(!err.message().is_empty());
        assert_eq!(err.default_severity(), Severity::Error);
        assert!(err.cause().is_none());
    }

    #[test]
    fn compile_error_equality_and_clone() {
        let a = CompileError::ReturnTypeFailure {
            name: "f".to_string(),
            reason: "boom".to_string(),
        };
        let b = a.clone();
        assert_eq!(a, b);
        let c = CompileError::ReturnTypeFailure {
            name: "g".to_string(),
            reason: "boom".to_string(),
        };
        assert_ne!(a, c);
    }
}
