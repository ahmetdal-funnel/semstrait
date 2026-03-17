//! Canonical column schema with ordinal-based field lookup.

use crate::data_type::DataType;
use crate::error::SchemaError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Canonical column schema. Ordinals are stable after construction.
/// Every PlanNode carries an output_schema; parent nodes derive field
/// references via schema.ordinal("name") — never by positional index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Schema {
    pub columns: Vec<SchemaColumn>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaColumn {
    pub name: String,
    pub data_type: DataType,
    pub nullable: bool,
    pub ordinal: u32,
}

impl Schema {
    /// Create a new schema from columns.
    /// Ordinals are assigned sequentially starting from 0.
    pub fn new(columns: Vec<SchemaColumn>) -> Self {
        Schema { columns }
    }

    /// Create a schema from a list of (name, data_type, nullable) tuples.
    pub fn from_fields(fields: Vec<(String, DataType, bool)>) -> Self {
        let columns = fields
            .into_iter()
            .enumerate()
            .map(|(i, (name, data_type, nullable))| SchemaColumn {
                name,
                data_type,
                nullable,
                ordinal: i as u32,
            })
            .collect();
        Schema { columns }
    }

    /// Get the ordinal for a column by name.
    /// Returns error if the column is not found.
    pub fn ordinal(&self, name: &str) -> Result<u32, SchemaError> {
        self.columns
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.ordinal)
            .ok_or_else(|| SchemaError::ColumnNotFound(name.to_string()))
    }

    /// Get a column by name.
    pub fn get_column(&self, name: &str) -> Option<&SchemaColumn> {
        self.columns.iter().find(|c| c.name == name)
    }

    /// Get a column by ordinal.
    pub fn get_column_by_ordinal(&self, ordinal: u32) -> Option<&SchemaColumn> {
        self.columns.iter().find(|c| c.ordinal == ordinal)
    }

    /// Join two schemas. Ordinals: [left | left.len + right].
    /// Left schema ordinals are preserved, right schema ordinals are shifted.
    pub fn join(left: &Schema, right: &Schema) -> Schema {
        let left_len = left.columns.len() as u32;

        let mut columns = left.columns.clone();

        for col in &right.columns {
            columns.push(SchemaColumn {
                name: col.name.clone(),
                data_type: col.data_type.clone(),
                nullable: col.nullable,
                ordinal: left_len + col.ordinal,
            });
        }

        Schema { columns }
    }

    /// Project the schema to keep only specified columns.
    /// Returns a new schema with only the named columns, preserving their ordinals.
    pub fn project(&self, keep: &[&str]) -> Schema {
        let keep_set: HashMap<&str, usize> = keep.iter()
            .enumerate()
            .map(|(i, &name)| (name, i))
            .collect();

        let mut columns: Vec<SchemaColumn> = self.columns
            .iter()
            .filter(|c| keep_set.contains_key(c.name.as_str()))
            .cloned()
            .collect();

        // Sort by the order specified in keep
        columns.sort_by_key(|c| keep_set.get(c.name.as_str()).unwrap());

        // Reassign ordinals sequentially
        for (i, col) in columns.iter_mut().enumerate() {
            col.ordinal = i as u32;
        }

        Schema { columns }
    }

    /// Emit a mapping from self's ordinals to target's ordinals.
    /// Returns a Vec where index i contains the target ordinal for self's column at ordinal i.
    /// Returns None for columns that don't exist in the target.
    pub fn emit_mapping(&self, target: &Schema) -> Vec<Option<u32>> {
        let target_map: HashMap<&str, u32> = target.columns
            .iter()
            .map(|c| (c.name.as_str(), c.ordinal))
            .collect();

        self.columns
            .iter()
            .map(|c| target_map.get(c.name.as_str()).copied())
            .collect()
    }

    /// Get the number of columns in the schema.
    pub fn len(&self) -> usize {
        self.columns.len()
    }

    /// Check if the schema is empty.
    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    /// Check if the schema contains a column with the given name.
    pub fn contains(&self, name: &str) -> bool {
        self.columns.iter().any(|c| c.name == name)
    }
}

impl SchemaColumn {
    /// Create a new schema column.
    pub fn new(name: String, data_type: DataType, nullable: bool, ordinal: u32) -> Self {
        SchemaColumn {
            name,
            data_type,
            nullable,
            ordinal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_schema() -> Schema {
        Schema::from_fields(vec![
            ("id".to_string(), DataType::Int32, false),
            ("name".to_string(), DataType::Utf8, false),
            ("amount".to_string(), DataType::Float64, false),
        ])
    }

    #[test]
    fn test_ordinal_lookup() {
        let schema = make_test_schema();

        assert_eq!(schema.ordinal("id").unwrap(), 0);
        assert_eq!(schema.ordinal("name").unwrap(), 1);
        assert_eq!(schema.ordinal("amount").unwrap(), 2);

        assert!(matches!(
            schema.ordinal("nonexistent"),
            Err(SchemaError::ColumnNotFound(_))
        ));
    }

    #[test]
    fn test_join() {
        let left = Schema::from_fields(vec![
            ("id".to_string(), DataType::Int32, false),
            ("name".to_string(), DataType::Utf8, false),
        ]);

        let right = Schema::from_fields(vec![
            ("amount".to_string(), DataType::Float64, false),
            ("date".to_string(), DataType::Date32, false),
        ]);

        let joined = Schema::join(&left, &right);

        assert_eq!(joined.len(), 4);
        assert_eq!(joined.ordinal("id").unwrap(), 0);
        assert_eq!(joined.ordinal("name").unwrap(), 1);
        assert_eq!(joined.ordinal("amount").unwrap(), 2); // left.len + 0
        assert_eq!(joined.ordinal("date").unwrap(), 3);   // left.len + 1
    }

    #[test]
    fn test_project() {
        let schema = make_test_schema();

        let projected = schema.project(&["name", "amount"]);

        assert_eq!(projected.len(), 2);
        assert_eq!(projected.ordinal("name").unwrap(), 0);
        assert_eq!(projected.ordinal("amount").unwrap(), 1);
        assert!(projected.ordinal("id").is_err());
    }

    #[test]
    fn test_project_preserves_order() {
        let schema = make_test_schema();

        let projected = schema.project(&["amount", "id"]);

        assert_eq!(projected.len(), 2);
        assert_eq!(projected.ordinal("amount").unwrap(), 0);
        assert_eq!(projected.ordinal("id").unwrap(), 1);
    }

    #[test]
    fn test_emit_mapping() {
        let source = Schema::from_fields(vec![
            ("id".to_string(), DataType::Int32, false),
            ("name".to_string(), DataType::Utf8, false),
            ("amount".to_string(), DataType::Float64, false),
        ]);

        let target = Schema::from_fields(vec![
            ("name".to_string(), DataType::Utf8, false),
            ("id".to_string(), DataType::Int32, false),
            ("extra".to_string(), DataType::Int32, false),
        ]);

        let mapping = source.emit_mapping(&target);

        assert_eq!(mapping.len(), 3);
        assert_eq!(mapping[0], Some(1)); // id -> ordinal 1 in target
        assert_eq!(mapping[1], Some(0)); // name -> ordinal 0 in target
        assert_eq!(mapping[2], None);    // amount not in target
    }

    #[test]
    fn test_get_column() {
        let schema = make_test_schema();

        let col = schema.get_column("name").unwrap();
        assert_eq!(col.name, "name");
        assert_eq!(col.data_type, DataType::Utf8);
        assert_eq!(col.ordinal, 1);

        assert!(schema.get_column("nonexistent").is_none());
    }

    #[test]
    fn test_get_column_by_ordinal() {
        let schema = make_test_schema();

        let col = schema.get_column_by_ordinal(1).unwrap();
        assert_eq!(col.name, "name");
        assert_eq!(col.data_type, DataType::Utf8);

        assert!(schema.get_column_by_ordinal(99).is_none());
    }

    #[test]
    fn test_contains() {
        let schema = make_test_schema();

        assert!(schema.contains("id"));
        assert!(schema.contains("name"));
        assert!(schema.contains("amount"));
        assert!(!schema.contains("nonexistent"));
    }

    #[test]
    fn test_serde_roundtrip() {
        let schema = make_test_schema();

        let json = serde_json::to_string(&schema).unwrap();
        let parsed: Schema = serde_json::from_str(&json).unwrap();

        assert_eq!(schema, parsed);
    }
}
