//! Parse-time error roster for the YAML authoring surface.
//!
//! Per spec `[14 §6](../../../../docs/design/foundations/14_expressions.md)`
//! and `[32 §9.2](../../../../docs/design/apis/32_semstrait_model.md)`.
//!
//! # Naming
//!
//! This iteration uses [`ParseError`] (no `Kind` suffix) for the new
//! expression-source parse surface only. The model crate's broader
//! `ParseErrorKind` rename remains a separate post-v1 sweep per
//! `STATUS.md` item Q. Integration with the model's existing
//! `ParseErrorKind` happens at call sites that surface a
//! `Diagnostic<ParseErrorKind>`; the new variants here are internal to
//! the expression-source machinery.

use semstrait_ir::ValidateError;
use thiserror::Error;

/// Parse-time errors raised while interpreting an `ExprSource<L>` from
/// YAML, or when consuming one in `parse_semantic` / `parse_physical`.
///
/// Construction-boundary failures emitted by `semstrait-ir`'s
/// `Tree::with_new_children` machinery flow in via [`ParseError::Ir`]
/// per the D.ii kind-nesting convention (`30 §7.4`).
#[non_exhaustive]
#[derive(Debug, Error, Clone, PartialEq)]
pub enum ParseError {
    /// Inline DSL parsing is deferred per `14 §6.3` (Inline grammar
    /// optional in v1). Calling `parse_semantic` / `parse_physical` on
    /// an `ExprSource::Inline(_)` value raises this variant until the
    /// Inline DSL lands.
    #[error("inline DSL is not implemented in this iteration")]
    InlineDslNotImplemented,

    /// A single-key map carried a tag that is not in the reserved-tag
    /// catalog (`14 §6.4.1`) and the function-registry look-aside is
    /// not yet wired in v1.
    #[error("unknown reserved tag: `{0}`")]
    UnknownTag(String),

    /// A reserved tag was encountered at a parse site that does not
    /// admit it (`14 §7`). Examples: `field` / `dim` / `measure` /
    /// `metric` / `key` at a physical-mapping site, or `window` at any
    /// expression site (Window is compile-emitted only).
    #[error("tag `{tag}` not allowed at {site} site")]
    TagNotAllowedAtSite {
        tag: String,
        site: &'static str,
    },

    /// A required field was missing from a structural-tag body.
    #[error("missing required field `{0}`")]
    MissingField(&'static str),

    /// A field value was rejected by the parser (e.g. an unknown
    /// `BinaryOpKind` spelling, a malformed accessor body).
    #[error("invalid value for field `{field}`: {reason}")]
    InvalidValue {
        field: &'static str,
        reason: String,
    },

    /// A single-key map carried more than one key (ambiguous tag).
    #[error("expected single-key tagged map, got {0} keys")]
    AmbiguousTag(usize),

    /// A YAML node had an unexpected shape — neither a string, a
    /// scalar, nor a single-key map.
    #[error("unexpected YAML shape: {0}")]
    UnexpectedShape(String),

    /// A construction-boundary failure raised by `semstrait-ir`'s
    /// `Tree::with_new_children` / `Rewriter::f_*` machinery while
    /// rebuilding the deserialized tree (D.ii kind-nesting per
    /// `30 §7.4`).
    #[error("ir validation: {0}")]
    Ir(#[from] ValidateError),

    /// A YAML-layer error encountered while materialising the source
    /// tree (e.g. malformed YAML, or a node where the deserializer
    /// expected a different primitive shape).
    #[error("yaml: {0}")]
    Yaml(String),
}
