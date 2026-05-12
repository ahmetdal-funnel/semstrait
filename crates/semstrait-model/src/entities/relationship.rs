//! `Relationship` and friends — `18 §2`.

use crate::entities::ai::AiContext;
use crate::error::build::ModelBuildErrorKind;
use crate::error::validate::ValidateErrorKind;
use crate::expr_ast::ExprSource;
use crate::types::DataKindName;
use bon::Builder;
use semstrait_core::diagnostic::Diagnostic;
use serde::{Deserialize, Serialize};

/// Unified `Relationship` struct shared between the root-level
/// `relationships:` list and `JoinsetBody.relationships`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Builder)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
#[builder(start_fn = builder, finish_fn = finalize)]
pub struct Relationship {
    #[builder(into)]
    pub name: String,

    #[builder(into)]
    pub from: DataKindName,
    #[builder(into)]
    pub to: DataKindName,

    /// Equi-join key pairs.
    #[serde(default)]
    #[builder(default)]
    pub keys: Vec<JoinKeyExprPair>,

    /// Optional residual predicate evaluated against the joined
    /// rowset. `None` means equi-join only per `keys`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub filter: Option<ExprSource>,

    /// REQUIRED at every authoring site (SR-E-4).
    pub cardinality: Cardinality,

    /// Author-asserted RI strength. Default `Assumed`.
    #[serde(default)]
    #[builder(default)]
    pub integrity: Integrity,

    /// Which side is preserved when matches are absent. Default
    /// derived from `(cardinality, integrity)` per `18 §2.7`. Required
    /// when `cardinality ∈ {OneToOne, ManyToMany}` per SR-E-13.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optional: Option<Optional>,

    /// Direction of filter propagation through the join. Default
    /// derived from `cardinality` per `18 §2.7`. Required when
    /// `cardinality ∈ {OneToOne, ManyToMany}` per SR-E-13.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cross_filter: Option<CrossFilter>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_context: Option<AiContext>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub description: Option<String>,
}

impl<S: relationship_builder::IsComplete> RelationshipBuilder<S> {
    /// Finalize and run cross-field invariant checks (SR-E-13 / SR-E-14
    /// per `18 §2.6`). Returns a single fail-fast diagnostic when a
    /// derived clause fires.
    pub fn build(self) -> Result<Relationship, Diagnostic<ModelBuildErrorKind>> {
        let r = self.finalize();
        validate_relationship_build(&r).map_err(Diagnostic::new)?;
        Ok(r)
    }
}

fn validate_relationship_build(r: &Relationship) -> Result<(), ModelBuildErrorKind> {
    use Cardinality::*;
    let symmetric = matches!(r.cardinality, OneToOne | ManyToMany);
    if symmetric {
        if r.optional.is_none() {
            return Err(ModelBuildErrorKind::Validate(
                ValidateErrorKind::RelationshipSymmetricCardinalityIncomplete {
                    relationship: r.name.clone(),
                    missing: "optional".to_string(),
                },
            ));
        }
        if r.cross_filter.is_none() {
            return Err(ModelBuildErrorKind::Validate(
                ValidateErrorKind::RelationshipSymmetricCardinalityIncomplete {
                    relationship: r.name.clone(),
                    missing: "cross_filter".to_string(),
                },
            ));
        }
    }
    if matches!(r.cardinality, ManyToMany) {
        if let Some(cf) = r.cross_filter {
            if matches!(cf, CrossFilter::Left | CrossFilter::Right) {
                return Err(ModelBuildErrorKind::Validate(
                    ValidateErrorKind::RelationshipManyToManyCrossFilterDirectional {
                        relationship: r.name.clone(),
                    },
                ));
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Cardinality {
    OneToOne,
    OneToMany,
    ManyToOne,
    ManyToMany,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Integrity {
    Enforced,
    #[default]
    Assumed,
    None,
}

/// Preserved-side enum on `Relationship.optional`. The value names the
/// side on the receiving end of preservation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Optional {
    /// Preserve neither side; drop unmatched rows on both sides.
    /// Derives Inner.
    None,
    /// Preserve the `from` (left) side; null-pad `to` for unmatched.
    /// Derives Left.
    Left,
    /// Preserve the `to` (right) side; null-pad `from` for unmatched.
    /// Derives Right.
    Right,
    /// Preserve both sides; null-pad whichever side lacks a match.
    /// Derives Full.
    Both,
}

/// Filter-flow enum on `Relationship.cross_filter`. The value names the
/// side that receives filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CrossFilter {
    /// No filter propagation between sides.
    None,
    /// `from` (left) side receives filters from `to`. Filter flow:
    /// `to → from`.
    Left,
    /// `to` (right) side receives filters from `from`. Filter flow:
    /// `from → to`.
    Right,
    /// Bidirectional propagation; both sides receive filters from the
    /// other.
    Both,
}

/// Hybrid equi-key grammar — one pair per equi-predicate. Authors
/// list a `JoinKeyExprPair` per equi-condition; the planner ANDs the
/// `filter:` predicate (if any) on top.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Builder)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
#[builder(start_fn = builder, finish_fn = build)]
pub struct JoinKeyExprPair {
    /// SemanticExpr on the `from` side. In the simplest case, a bare
    /// Semantic name.
    #[builder(into)]
    pub from: ExprSource,
    /// SemanticExpr on the `to` side. Symmetric.
    #[builder(into)]
    pub to: ExprSource,
}

impl JoinKeyExprPair {
    /// Convenience constructor for the common bare-Semantic-field case.
    /// Both sides are wrapped in `ExprSource::Inline(name)`.
    pub fn fields(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: ExprSource::Inline(from.into()),
            to: ExprSource::Inline(to.into()),
        }
    }
}

/// Operational join kind. **Not authored** — derived at compile from
/// `Relationship.optional` per the table in `18 §2.9`. Carried on the
/// SemanticManifest's `ResolvedRelationship`. Listed here only so that
/// downstream code linked against `semstrait-model` can name the kind
/// without depending on the manifest crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
}

