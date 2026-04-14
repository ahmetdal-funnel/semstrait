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

impl Keys {
    /// Collect all unique column names across primary, unique, and foreign keys.
    pub fn all_column_names(&self) -> Vec<String> {
        let mut names = std::collections::HashSet::new();
        if let Some(ref primary) = self.primary {
            names.extend(primary.iter().cloned());
        }
        if let Some(ref unique) = self.unique {
            for uc in unique {
                names.extend(uc.columns.iter().cloned());
            }
        }
        if let Some(ref foreign) = self.foreign {
            for fk in foreign {
                names.extend(fk.columns.iter().cloned());
            }
        }
        names.into_iter().collect()
    }
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
