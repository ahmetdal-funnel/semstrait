//! Column definitions for fact tables

use super::types::DataType;
use serde::Deserialize;

/// A column in the fact table with its data type
#[derive(Debug, Deserialize, Clone)]
pub struct Column {
    pub name: String,
    /// Data type (e.g., i32, i64, f64, decimal(31, 7), string)
    #[serde(rename = "type")]
    pub data_type: DataType,
}

impl Column {
    /// Get the data type of this column
    pub fn data_type(&self) -> &DataType {
        &self.data_type
    }
}
