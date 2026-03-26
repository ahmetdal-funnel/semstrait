//! Data format types for source resolution.

use serde::{Deserialize, Serialize};

/// Physical data format of a source.
///
/// Determines how metadata is extracted and how connectors register/read data:
/// - `Iceberg` — catalog-managed table; data is Parquet, access via Iceberg metadata
/// - `Parquet` — direct Parquet file access
/// - `Csv` — direct CSV file access
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataFormat {
    Iceberg,
    Parquet,
    Csv,
}

impl std::fmt::Display for DataFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataFormat::Iceberg => write!(f, "iceberg"),
            DataFormat::Parquet => write!(f, "parquet"),
            DataFormat::Csv => write!(f, "csv"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_format_serde_roundtrip() {
        let formats = vec![DataFormat::Iceberg, DataFormat::Parquet, DataFormat::Csv];
        for fmt in formats {
            let json = serde_json::to_string(&fmt).unwrap();
            let back: DataFormat = serde_json::from_str(&json).unwrap();
            assert_eq!(fmt, back);
        }
    }

    #[test]
    fn test_data_format_display() {
        assert_eq!(DataFormat::Iceberg.to_string(), "iceberg");
        assert_eq!(DataFormat::Parquet.to_string(), "parquet");
        assert_eq!(DataFormat::Csv.to_string(), "csv");
    }
}
