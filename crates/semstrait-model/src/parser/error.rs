//! Unified parse-time error roster for the YAML authoring surface
//! (block expressions + `semantic_mapping:` values).
//!
//! Per spec `[14 §6](../../../../docs/design/foundations/14_expressions.md)`
//! and `[32 §9.2](../../../../docs/design/apis/32_semstrait_model.md)`.
//!
//! Replaces `expr_source/error.rs` after Phase 8's parser/ refactor —
//! every variant from the old `ParseError` is preserved (rename pass
//! only). New variants:
//!
//! - [`ParseError::InvalidToken`] — leaf-token rules from
//!   `[parser/token.rs](super::token)` (single-quote literal, double-
//!   quote dotted name, accessor-split rules).
//! - [`ParseError::InvalidMappingValue`] — `semantic_mapping:` arms
//!   that don't match the bare-`Column` / `lit:` / `expr:` rule.

use semstrait_ir::ValidateError;
use thiserror::Error;

/// Parse-time errors raised while interpreting an `ExprSource<L>` from
/// YAML, a `semantic_mapping:` value, or a leaf token.
#[non_exhaustive]
#[derive(Debug, Error, Clone, PartialEq)]
pub enum ParseError {
    /// Inline DSL parsing is deferred per `14 §6.3`. Calling
    /// `parse_semantic` / `parse_physical` on an `ExprSource::Inline(_)`
    /// value raises this variant until the Inline DSL lands.
    #[error("inline DSL is not implemented in this iteration")]
    InlineDslNotImplemented,

    /// A single-key map carried a tag that is not in the closed sugar
    /// roster (`14 §6.4.1`). Unknown function names are rejected here —
    /// there is no `function_call:` author-facing escape hatch.
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

    /// A field value was rejected by the parser.
    #[error("invalid value for field `{field}`: {reason}")]
    InvalidValue {
        field: &'static str,
        reason: String,
    },

    /// A leaf token violated the tokenizer rules (per
    /// `[parser/token.rs](super::token)`): unterminated quoted form,
    /// empty identifier, accessor split malformed, etc.
    #[error("invalid token `{raw}`: {reason}")]
    InvalidToken {
        raw: String,
        reason: &'static str,
    },

    /// A `semantic_mapping:` value did not match `bare scalar = Column`,
    /// `{lit: ...}`, or `{expr: ...}`.
    #[error("invalid semantic_mapping value: {0}")]
    InvalidMappingValue(String),

    /// A single-key map carried more than one key (ambiguous tag).
    #[error("expected single-key tagged map, got {0} keys")]
    AmbiguousTag(usize),

    /// A YAML node had an unexpected shape — neither a string, a
    /// scalar, nor a single-key map.
    #[error("unexpected YAML shape: {0}")]
    UnexpectedShape(String),

    /// A construction-boundary failure raised by `semstrait-ir`'s
    /// `Tree::with_new_children` machinery while rebuilding the
    /// deserialized tree (D.ii kind-nesting per `30 §7.4`).
    #[error("ir validation: {0}")]
    Ir(#[from] ValidateError),

    /// A YAML-layer error encountered while materialising the source
    /// tree.
    #[error("yaml: {0}")]
    Yaml(String),
}
