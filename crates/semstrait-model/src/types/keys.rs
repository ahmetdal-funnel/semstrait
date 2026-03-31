//! Key and constraint types for semantic models.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Keys {
    #[serde(default)]
    pub primary: Option<Vec<String>>,
    #[serde(default)]
    pub unique: Option<Vec<UniqueConstraint>>,
    #[serde(default)]
    pub foreign: Option<Vec<ForeignKey>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UniqueConstraint {
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ForeignKey {
    pub columns: Vec<String>,
    pub reference: String,
    pub cardinality: Cardinality,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Cardinality {
    OneToOne,
    OneToMany,
    ManyToOne,
    ManyToMany,
}
