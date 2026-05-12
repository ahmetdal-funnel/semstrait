//! `Keys`, `KeyDecl`, `ForeignKeyDecl` — `18 §9`.

use crate::types::{DataKindName, SemanticsName};
use bon::Builder;
use serde::{Deserialize, Serialize};

/// Per-DataKind key block. All three families are optional; absence
/// means "no declaration", not "the data kind has no keys at all"
/// (catalog metadata may still surface keys at compile).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Builder)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
#[builder(start_fn = builder, finish_fn = build)]
pub struct Keys {
    /// At most one primary key declaration per DataKind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<KeyDecl>,

    /// Additional unique keys beyond the primary.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[builder(default)]
    pub unique: Vec<KeyDecl>,

    /// Foreign keys — references to other DataKinds' primary / unique
    /// keys. Consumed by the relationship graph (`16 §11`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[builder(default)]
    pub foreign: Vec<ForeignKeyDecl>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Builder)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
#[builder(start_fn = builder, finish_fn = build)]
pub struct KeyDecl {
    /// Bare Semantic names — no physical column references at the model
    /// surface. Resolution through `semantic_mapping` happens at compile.
    pub columns: Vec<SemanticsName>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Builder)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
#[builder(start_fn = builder, finish_fn = build)]
pub struct ForeignKeyDecl {
    pub columns: Vec<SemanticsName>,
    /// The target DataKind whose primary / unique key is referenced.
    #[builder(into)]
    pub references: DataKindName,
    /// The target DataKind's key columns — bare Semantic names.
    pub target_columns: Vec<SemanticsName>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub name: Option<String>,
}
