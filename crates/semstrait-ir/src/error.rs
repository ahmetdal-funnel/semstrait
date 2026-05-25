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
//! - [`IrErrorKind`] — plan-tree boundary diagnostics raised by
//!   `SemanticPlan::validate`, `transform`, and the Substrait codec
//!   (`EnginePlan::to_bytes`). Per spec `35 §16.3`.
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

    /// `Name::new` rejected an empty identifier. Per `35 §11.4`.
    #[error("Name cannot be empty")]
    EmptyName,

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

/// Plan-tree boundary diagnostic raised by `SemanticPlan::validate`,
/// `PlanNode::transform`, and the Substrait codec
/// (`EnginePlan::to_bytes`). Per spec `35 §16.3`.
///
/// **Scoping override (Q-PLAN-14, 2026-05-25).** The full §16.3 catalog
/// lists 14 variants spanning structural shape, type-resolution, and
/// schema-mismatch failures. Most of those (e.g. `JoinKeyTypeMismatch`,
/// `UnresolvedType`, `DuplicateAggName`, `UnionSchemaMismatch`) are
/// upstream-of-IR concerns: they describe planner / manifest-compile
/// failures that should be caught before a `SemanticPlan` is constructed.
/// `semstrait-ir` v1 carries only the three variants whose production
/// site is structurally inside `35`:
///
/// - [`StructuralViolation`] — `validate()` post-order walk caught a
///   shape-only invariant break that did not abort an earlier
///   `with_new_children` call (e.g. `Union.inputs.len() < 2`,
///   `Filter.predicate` non-Boolean as a structural fact, not a type
///   diagnostic). Composite envelope; consumers read `kind` for
///   sub-classification.
/// - [`DanglingReference`] — a `Name` on a `JoinNode.on`,
///   `AggNode.group_by`, or `SortNode.keys` does not resolve to any
///   column in the corresponding child schema.
/// - [`SubstraitCodecError`] — `EnginePlan::to_bytes` /
///   `from_bytes` failed at the prost / Substrait boundary; wraps the
///   underlying message context.
///
/// Adding variants is MINOR per `30 §2.2`; widening this enum is
/// expected as planner / manifest layers re-route their own diagnostics
/// up to caller boundaries instead of through `35`.
///
/// [`StructuralViolation`]: IrErrorKind::StructuralViolation
/// [`DanglingReference`]: IrErrorKind::DanglingReference
/// [`SubstraitCodecError`]: IrErrorKind::SubstraitCodecError
#[non_exhaustive]
#[derive(Debug, Error, Clone, PartialEq)]
pub enum IrErrorKind {
    /// Structural shape invariant violated by `SemanticPlan::validate`'s
    /// post-order walk. Per `35 §14.3` / `§16.3`.
    ///
    /// `kind` is a stable `&'static str` discriminator — `"union_arity"`,
    /// `"filter_predicate"`, etc. — so callers can branch without
    /// re-parsing `reason`. Adding a new `kind` token is MINOR.
    #[error("structural violation [{kind}]: {reason}")]
    StructuralViolation {
        kind: &'static str,
        reason: String,
    },

    /// A column [`crate::primitives::Name`] referenced by `JoinNode.on`,
    /// `AggNode.group_by`, or `SortNode.keys` does not resolve to any
    /// column in the corresponding child schema. Per `35 §14.3` / `§16.3`.
    ///
    /// `available` lists the candidate names from the child schema for
    /// diagnostic display. `node_kind` records which plan-node variant
    /// raised the diagnostic (`"join"`, `"agg"`, `"sort"`).
    #[error(
        "dangling reference on {node_kind}: column `{}` not found in child schema (available: [{}])",
        name.as_str(),
        available.iter().map(|n| n.as_str()).collect::<Vec<_>>().join(", "),
    )]
    DanglingReference {
        node_kind: &'static str,
        name: crate::primitives::Name,
        available: Vec<crate::primitives::Name>,
    },

    /// Substrait codec failure raised by `EnginePlan::to_bytes` /
    /// `from_bytes`. Per `35 §16.3`. Wraps the underlying
    /// `prost::EncodeError` / `prost::DecodeError` message — we hold a
    /// `String` rather than the typed prost error so this enum can
    /// remain `Clone + PartialEq` without leaking the prost dependency
    /// shape into downstream consumers.
    #[error("substrait codec error [{phase}]: {reason}")]
    SubstraitCodecError {
        phase: &'static str,
        reason: String,
    },
}

impl Diagnose for IrErrorKind {
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
    fn validate_error_displays_empty_name() {
        let err = ValidateError::EmptyName;
        let msg = format!("{}", err);
        assert!(msg.contains("Name"));
        assert!(msg.contains("empty"));
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

    // ── IrErrorKind ──────────────────────────────────────────────────

    #[test]
    fn ir_error_displays_structural_violation() {
        let err = IrErrorKind::StructuralViolation {
            kind: "union_arity",
            reason: "expected ≥2 inputs, got 1".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("structural violation"));
        assert!(msg.contains("union_arity"));
        assert!(msg.contains("expected ≥2 inputs"));
    }

    #[test]
    fn ir_error_displays_dangling_reference() {
        use crate::primitives::Name;
        let err = IrErrorKind::DanglingReference {
            node_kind: "join",
            name: Name::new("xyz").unwrap(),
            available: vec![
                Name::new("order_id").unwrap(),
                Name::new("amount").unwrap(),
            ],
        };
        let msg = format!("{}", err);
        assert!(msg.contains("dangling reference"));
        assert!(msg.contains("join"));
        assert!(msg.contains("xyz"));
        assert!(msg.contains("order_id"));
        assert!(msg.contains("amount"));
    }

    #[test]
    fn ir_error_displays_dangling_reference_with_empty_schema() {
        use crate::primitives::Name;
        let err = IrErrorKind::DanglingReference {
            node_kind: "sort",
            name: Name::new("missing").unwrap(),
            available: vec![],
        };
        let msg = format!("{}", err);
        assert!(msg.contains("missing"));
        assert!(msg.contains("[]") || msg.contains("available"));
    }

    #[test]
    fn ir_error_displays_substrait_codec_error() {
        let err = IrErrorKind::SubstraitCodecError {
            phase: "encode",
            reason: "buffer overflow".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("substrait codec"));
        assert!(msg.contains("encode"));
        assert!(msg.contains("buffer overflow"));
    }

    #[test]
    fn ir_error_implements_diagnose() {
        let err = IrErrorKind::StructuralViolation {
            kind: "filter_predicate",
            reason: "non-Boolean".to_string(),
        };
        assert!(!err.message().is_empty());
        assert_eq!(err.severity_default(), Severity::Error);
        assert!(err.cause().is_none());
    }

    #[test]
    fn ir_error_equality_and_clone() {
        let a = IrErrorKind::StructuralViolation {
            kind: "union_arity",
            reason: "x".to_string(),
        };
        let b = a.clone();
        assert_eq!(a, b);
        let c = IrErrorKind::StructuralViolation {
            kind: "filter_predicate",
            reason: "x".to_string(),
        };
        assert_ne!(a, c, "kind discriminator participates in equality");
    }

    #[test]
    fn ir_error_variants_distinguish_by_identity() {
        // Variant identity (not stringly-typed codes) drives equality
        // per `30 §5.4`. Two structurally-different variants with the
        // same `reason` MUST compare unequal.
        let a = IrErrorKind::StructuralViolation {
            kind: "union_arity",
            reason: "boom".to_string(),
        };
        let b = IrErrorKind::SubstraitCodecError {
            phase: "decode",
            reason: "boom".to_string(),
        };
        assert_ne!(a, b);
    }
}
