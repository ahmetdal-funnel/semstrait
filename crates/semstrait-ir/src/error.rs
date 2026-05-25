//! Error types for IR operations.
//!
//! Hosts the narrow ir-emitted error kinds introduced by the
//! second-cascade landing (`STATUS.md` item Q):
//!
//! - [`ValidateError`] — raised by [`crate::tree::Tree::with_new_children`]
//!   and [`crate::tree::Rewriter`] callbacks, plus
//!   [`crate::functions::CanonicalFn::new`] name-grammar rejections.
//!   Per spec `35 §16.1` / `14 §3.1`.
//! - [`CompileError`] — raised by `ReturnTypeRule::Custom` callbacks wired
//!   into `FunctionSpec` and by registry self-consistency checks.
//!   Per spec `35 §16.2` / `14a §2`.
//!
//! Downstream stages embed via D.ii kind-nesting per `30 §7.4` —
//! `model::ValidateError` carries `Ir(ir::ValidateError)`;
//! `manifest::CompileError` carries `Ir(ir::CompileError)`.

use semstrait_common::diagnostic::{Diagnose, Severity};
use thiserror::Error;

use crate::functions::CanonicalFn;
use crate::types::DataType;

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

    /// `CanonicalFn::new` rejected the supplied name. Per `14 §6.5`
    /// identifier grammar `[A-Za-z_][A-Za-z0-9_]*`; canonical names are
    /// lowercase ASCII per `14a §2.3`.
    #[error("invalid canonical function name `{supplied}`: {reason}")]
    InvalidCanonicalFn {
        supplied: String,
        reason: &'static str,
    },
}

impl Diagnose for ValidateError {
    fn message(&self) -> String {
        // `thiserror`'s Display impl gives a per-variant message; reuse it.
        format!("{}", self)
    }

    fn severity_default(&self) -> Severity {
        Severity::Error
    }
}

/// Function-resolution diagnostic raised by `ReturnTypeRule::Custom`
/// callbacks wired into `FunctionSpec`, and by registry self-consistency
/// checks. Per spec `35 §16.2` / `14a §2`.
///
/// The wider compile-stage error surface (unknown references, ambiguous
/// paths, cycles, type-inference failures) lives in
/// `semstrait-manifest::CompileError` and embeds `Ir(ir::CompileError)`
/// via D.ii kind-nesting per `30 §7.4`.
#[non_exhaustive]
#[derive(Debug, Error, Clone, PartialEq)]
pub enum CompileError {
    /// `ReturnTypeRule::Custom` callback declined to produce a return
    /// type for the supplied argument types. Per `35 §16.2`.
    #[error("function `{}`: custom return-type rule rejected — {reason}", fn_name.as_str())]
    CustomRuleRejected {
        fn_name: CanonicalFn,
        args: Vec<DataType>,
        reason: String,
    },

    /// A registered `FunctionSpec` failed its own internal consistency
    /// check (e.g. signature overlaps another in the same registry,
    /// empty `signatures` vec). Per `35 §16.2`.
    #[error("function `{}`: spec inconsistent — {reason}", fn_name.as_str())]
    SpecInconsistent {
        fn_name: CanonicalFn,
        reason: String,
    },
}

impl Diagnose for CompileError {
    fn message(&self) -> String {
        format!("{}", self)
    }

    fn severity_default(&self) -> Severity {
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
        assert_eq!(err.severity_default(), Severity::Error);
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
    fn validate_error_displays_invalid_canonical_fn() {
        let err = ValidateError::InvalidCanonicalFn {
            supplied: "foo bar".to_string(),
            reason: "non-grammar character",
        };
        let msg = format!("{}", err);
        assert!(msg.contains("foo bar"));
        assert!(msg.contains("non-grammar character"));
    }

    #[test]
    fn compile_error_displays_custom_rule_rejected() {
        let err = CompileError::CustomRuleRejected {
            fn_name: CanonicalFn::new("coalesce").unwrap(),
            args: vec![DataType::Integer],
            reason: "no arguments supplied".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("coalesce"));
        assert!(msg.contains("no arguments supplied"));
    }

    #[test]
    fn compile_error_displays_spec_inconsistent() {
        let err = CompileError::SpecInconsistent {
            fn_name: CanonicalFn::new("upper").unwrap(),
            reason: "empty signatures".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("upper"));
        assert!(msg.contains("empty signatures"));
    }

    #[test]
    fn compile_error_implements_diagnose() {
        let err = CompileError::CustomRuleRejected {
            fn_name: CanonicalFn::new("x").unwrap(),
            args: vec![],
            reason: "y".to_string(),
        };
        assert!(!err.message().is_empty());
        assert_eq!(err.severity_default(), Severity::Error);
        assert!(err.cause().is_none());
    }

    #[test]
    fn compile_error_equality_and_clone() {
        let a = CompileError::CustomRuleRejected {
            fn_name: CanonicalFn::new("f").unwrap(),
            args: vec![DataType::Long],
            reason: "boom".to_string(),
        };
        let b = a.clone();
        assert_eq!(a, b);
        let c = CompileError::CustomRuleRejected {
            fn_name: CanonicalFn::new("g").unwrap(),
            args: vec![DataType::Long],
            reason: "boom".to_string(),
        };
        assert_ne!(a, c);
    }
}
