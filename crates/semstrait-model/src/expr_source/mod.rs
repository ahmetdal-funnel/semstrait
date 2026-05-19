//! YAML authoring surface for expressions per spec
//! `[14 §6](../../../docs/design/foundations/14_expressions.md)` /
//! `[32 §6](../../../docs/design/apis/32_semstrait_model.md)`.
//!
//! [`ExprSource<L>`] is discriminated by serde shape:
//!
//! - `String` value → [`ExprSource::Inline`].
//! - `Mapping` value → [`ExprSource::Block`], dispatched by the
//!   reserved-tag catalog implemented in [`crate::parser::block`].
//!
//! No separate `ExprBlock` AST per spec item Q (`STATUS.md`): the YAML
//! shape **is** [`semstrait_ir::Expr<L>`] deserialized via the
//! [`Deserialize`](serde::Deserialize) impl below, which delegates to
//! [`crate::parser::block::parse_block`].
//!
//! # Parse-site dispatch
//!
//! - [`parse_semantic`] consumes an `ExprSource<SemanticLeaf>` and
//!   produces a `SemanticExpr`. Bare identifiers resolve to
//!   `Field(name)`.
//! - [`parse_physical`] consumes an `ExprSource<PhysicalLeaf>` and
//!   produces a `PhysicalExpr`. Bare identifiers resolve to
//!   `Column(name)`. Semantic tags (`field` / `dim` / `measure` /
//!   `metric` / `key`) are rejected by the deserializer's
//!   [`crate::parser::leaf::LeafResolver`] impl for `PhysicalLeaf`.

use crate::parser::block::parse_block;
use crate::parser::error::ParseError;
use crate::parser::leaf::LeafResolver;
use semstrait_ir::{Expr, ExprLeaf, PhysicalExpr, PhysicalLeaf, SemanticExpr, SemanticLeaf};
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};
use serde_yaml::Value;

/// YAML-authored expression source per spec `14 §6.1`.
///
/// Two interchangeable forms:
///
/// - [`ExprSource::Inline`] — constrained SQL-like DSL string.
///   **Out of scope for this iteration**; `parse_semantic` /
///   `parse_physical` raise [`ParseError::InlineDslNotImplemented`]
///   when called on `Inline(_)`.
/// - [`ExprSource::Block`] — structured YAML tree deserialized
///   directly into [`Expr<L>`] via the [`Deserialize`] impl below.
///
/// Generic in the leaf set `L`. The two canonical instantiations are
/// `ExprSource<SemanticLeaf>` (semantic sites) and
/// `ExprSource<PhysicalLeaf>` (physical-mapping sites).
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum ExprSource<L: ExprLeaf> {
    /// Inline DSL string. Inline grammar is deferred per `14 §6.3`.
    Inline(String),
    /// Structured YAML tree — `Expr<L>` deserialized via the
    /// reserved-tag catalog (`14 §6.4.1`).
    Block(Expr<L>),
}

impl<L: ExprLeaf> ExprSource<L> {
    /// Returns the underlying [`Expr<L>`] when this is a [`Block`],
    /// else [`ParseError::InlineDslNotImplemented`].
    ///
    /// [`Block`]: ExprSource::Block
    pub fn into_expr(self) -> Result<Expr<L>, ParseError> {
        match self {
            ExprSource::Block(e) => Ok(e),
            ExprSource::Inline(_) => Err(ParseError::InlineDslNotImplemented),
        }
    }

    /// Returns `true` for [`ExprSource::Inline`] values.
    pub fn is_inline(&self) -> bool {
        matches!(self, ExprSource::Inline(_))
    }

    /// Returns `true` for [`ExprSource::Block`] values.
    pub fn is_block(&self) -> bool {
        matches!(self, ExprSource::Block(_))
    }
}

// ── Deserialize: delegates to `parser::block::parse_block` ──────────────

impl<'de, L> Deserialize<'de> for ExprSource<L>
where
    L: LeafResolver,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        parse_block::<L>(&value).map_err(|e| serde::de::Error::custom(e.to_string()))
    }
}

// ── Serialize: untagged — Inline writes a string, Block writes the ──
//                          single-key tag of its root variant.
//
// The Serialize impl is intentionally narrow: it preserves the `Inline`
// string round-trip exactly, and emits a placeholder `{ block: <Debug> }`
// mapping for the Block arm. The full reverse-of-deserialize Serialize
// (round-trip the reserved-tag catalog) is deferred to a later phase
// (the `dump_model` round-trip guard already lives in `model::io` per
// `32 §10.4.3`); call sites that round-trip canonical YAML go through
// the loader / dumper, not through this leaf serializer.
impl<L: ExprLeaf> Serialize for ExprSource<L> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            ExprSource::Inline(s) => serializer.serialize_str(s),
            ExprSource::Block(expr) => {
                // Block round-trip is debug-only in this iteration —
                // structural-shape diagnostics still need *some*
                // serialized form so consumers like
                // `Diagnostic<...>` Debug printing don't blow up. The
                // `dump_model` round-trip guard owns the strict
                // canonical form.
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("block", &format!("{expr:?}"))?;
                map.end()
            }
        }
    }
}

/// Parse a semantic-site `ExprSource` into a [`SemanticExpr`].
///
/// Bare identifiers inside the source resolve to `Field(name)` per
/// `14 §6.5`. Inline sources raise
/// [`ParseError::InlineDslNotImplemented`].
pub fn parse_semantic(src: &ExprSource<SemanticLeaf>) -> Result<SemanticExpr, ParseError> {
    match src {
        ExprSource::Block(e) => Ok(e.clone()),
        ExprSource::Inline(_) => Err(ParseError::InlineDslNotImplemented),
    }
}

/// Parse a physical-mapping-site `ExprSource` into a [`PhysicalExpr`].
///
/// Bare identifiers resolve to `Column(name)` per `14 §6.5`. The
/// deserializer rejects semantic tags (`field` / `dim` / `measure` /
/// `metric` / `key`) at this site — those tags don't reach this
/// function because they fail during `serde::Deserialize`.
pub fn parse_physical(src: &ExprSource<PhysicalLeaf>) -> Result<PhysicalExpr, ParseError> {
    match src {
        ExprSource::Block(e) => Ok(e.clone()),
        ExprSource::Inline(_) => Err(ParseError::InlineDslNotImplemented),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use semstrait_ir::{ColumnRef, Literal, PhysicalLeaf, SemanticLeaf, SemanticsName};

    #[test]
    fn into_expr_returns_block() {
        let leaf = Expr::Leaf(SemanticLeaf::Field(SemanticsName("x".into())));
        let src = ExprSource::Block(leaf.clone());
        assert_eq!(src.into_expr().unwrap(), leaf);
    }

    #[test]
    fn into_expr_rejects_inline() {
        let src: ExprSource<SemanticLeaf> = ExprSource::Inline("x + 1".into());
        let err = src.into_expr().unwrap_err();
        assert_eq!(err, ParseError::InlineDslNotImplemented);
    }

    #[test]
    fn parse_semantic_rejects_inline() {
        let src: ExprSource<SemanticLeaf> = ExprSource::Inline("x + 1".into());
        let err = parse_semantic(&src).unwrap_err();
        assert_eq!(err, ParseError::InlineDslNotImplemented);
    }

    #[test]
    fn parse_semantic_returns_block_clone() {
        let leaf = Expr::Leaf(SemanticLeaf::Literal(Literal::Integer(42)));
        let src = ExprSource::Block(leaf.clone());
        assert_eq!(parse_semantic(&src).unwrap(), leaf);
    }

    #[test]
    fn parse_physical_rejects_inline() {
        let src: ExprSource<PhysicalLeaf> = ExprSource::Inline("amount".into());
        let err = parse_physical(&src).unwrap_err();
        assert_eq!(err, ParseError::InlineDslNotImplemented);
    }

    #[test]
    fn parse_physical_returns_block_clone() {
        let leaf = Expr::Leaf(PhysicalLeaf::Column(ColumnRef("amount".into())));
        let src = ExprSource::Block(leaf.clone());
        assert_eq!(parse_physical(&src).unwrap(), leaf);
    }

    #[test]
    fn is_inline_and_is_block_predicates() {
        let inline: ExprSource<SemanticLeaf> = ExprSource::Inline("x".into());
        assert!(inline.is_inline());
        assert!(!inline.is_block());
        let block: ExprSource<SemanticLeaf> =
            ExprSource::Block(Expr::Leaf(SemanticLeaf::Literal(Literal::Null)));
        assert!(block.is_block());
        assert!(!block.is_inline());
    }
}
