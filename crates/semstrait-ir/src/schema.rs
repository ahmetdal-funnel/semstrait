//! Schema types for tracking column names and types in the IR

use semstrait_core::DataType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A field in a schema (column name + type)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub data_type: DataType,
    pub nullable: bool,
}

impl Field {
    pub fn new(name: impl Into<String>, data_type: DataType) -> Self {
        Self {
            name: name.into(),
            data_type,
            nullable: true,
        }
    }

    pub fn with_nullable(mut self, nullable: bool) -> Self {
        self.nullable = nullable;
        self
    }
}

/// Schema represents the output columns of a PlanNode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schema {
    pub fields: Vec<Field>,
    #[serde(skip)]
    index: HashMap<String, usize>,
}

impl Schema {
    pub fn new(fields: Vec<Field>) -> Self {
        let index: HashMap<String, usize> = fields
            .iter()
            .enumerate()
            .map(|(i, f)| (f.name.clone(), i))
            .collect();
        Self { fields, index }
    }

    pub fn empty() -> Self {
        Self {
            fields: Vec::new(),
            index: HashMap::new(),
        }
    }

    /// Find the ordinal (index) of a field by name
    pub fn ordinal(&self, name: &str) -> Option<usize> {
        self.index.get(name).copied()
    }

    /// Get field by index
    pub fn field(&self, index: usize) -> Option<&Field> {
        self.fields.get(index)
    }

    /// Number of fields
    pub fn len(&self) -> usize {
        self.fields.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }
}

impl PartialEq for Schema {
    fn eq(&self, other: &Self) -> bool {
        // Only compare fields, since index is derived from fields
        self.fields == other.fields
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_ordinal_lookup() {
        let schema = Schema::new(vec![
            Field::new("a", DataType::Integer),
            Field::new("b", DataType::Number),
            Field::new("c", DataType::String),
        ]);
        assert_eq!(schema.ordinal("a"), Some(0));
        assert_eq!(schema.ordinal("b"), Some(1));
        assert_eq!(schema.ordinal("c"), Some(2));
        assert_eq!(schema.ordinal("d"), None);
    }

    #[test]
    fn test_schema_empty() {
        let schema = Schema::empty();
        assert!(schema.is_empty());
        assert_eq!(schema.len(), 0);
        assert_eq!(schema.ordinal("any"), None);
    }

    #[test]
    fn test_schema_equality() {
        let schema1 = Schema::new(vec![
            Field::new("a", DataType::Integer),
            Field::new("b", DataType::Number),
        ]);
        let schema2 = Schema::new(vec![
            Field::new("a", DataType::Integer),
            Field::new("b", DataType::Number),
        ]);
        assert_eq!(schema1, schema2);
    }

    #[test]
    fn test_schema_clone_preserves_index() {
        let schema1 = Schema::new(vec![
            Field::new("x", DataType::Integer),
            Field::new("y", DataType::String),
        ]);
        let schema2 = schema1.clone();
        assert_eq!(schema2.ordinal("x"), Some(0));
        assert_eq!(schema2.ordinal("y"), Some(1));
    }
}
