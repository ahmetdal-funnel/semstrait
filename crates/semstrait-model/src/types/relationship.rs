//! Relationship types for semantic models.

use serde::{Deserialize, Serialize};

use super::keys::Cardinality;

/// Top-level relationship (joins between datasets and/or entities).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Relationship {
    pub name: String,
    pub from: String,
    pub to: String,
    #[serde(rename = "type")]
    pub join_type: JoinType,
    pub columns: Vec<JoinColumnPair>,
    pub cardinality: Cardinality,
}

/// Entity-internal relationship (used for joinset join paths).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JoinRelationship {
    pub name: String,
    pub from: String,
    pub to: String,
    #[serde(rename = "type")]
    pub join_type: JoinType,
    pub columns: Vec<JoinColumnPair>,
    pub cardinality: Cardinality,
}

/// Backward-compat alias: `DataKindRelationship` → `JoinRelationship`
pub type DataKindRelationship = JoinRelationship;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JoinColumnPair {
    pub from: String,
    pub to: String,
}
