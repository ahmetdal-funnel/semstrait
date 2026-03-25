//! Catalog snapshot types — physical metadata captured at compile time.
//!
//! These types represent the catalog state pinned during manifest compilation
//! (steps 10-13). They provide authoritative physical bindings for:
//! - Column schemas with data types
//! - Iceberg partition specs with temporal grain inference
//! - Snapshot IDs for reproducibility
//! - Table locations for connector auto-registration

use serde::{Deserialize, Serialize};
use semstrait_core::DataType;
use semstrait_model::TemporalGrain;
use std::collections::HashMap;

/// Catalog state captured at compilation time.
///
/// Contains per-table metadata snapshots. When present on `CompiledManifest`,
/// enables type validation, partition-aware grain inference, and reproducible builds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogSnapshot {
    /// Per-table metadata, keyed by fully qualified table name.
    pub tables: HashMap<String, TableSnapshot>,
    /// Timestamp when catalog metadata was fetched.
    pub captured_at: chrono::DateTime<chrono::Utc>,
}

/// Metadata snapshot for a single catalog table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSnapshot {
    /// Fully qualified table reference (e.g., "catalog.namespace.table").
    pub fqn: String,
    /// Column schema at compile time.
    pub columns: Vec<ResolvedColumn>,
    /// Iceberg-specific metadata (partition specs, snapshot ID, location).
    pub iceberg: Option<IcebergMetadata>,
}

/// A resolved physical column from the catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedColumn {
    pub name: String,
    pub data_type: DataType,
    pub nullable: bool,
    pub comment: Option<String>,
    /// Iceberg field ID (for partition spec resolution).
    pub field_id: Option<i32>,
}

/// Iceberg-specific table metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IcebergMetadata {
    /// Current snapshot ID from the Iceberg table.
    pub snapshot_id: i64,
    /// Partition spec fields with inferred temporal grains.
    pub partition_spec: Vec<PartitionField>,
    /// Iceberg format version (1 or 2).
    pub format_version: Option<u32>,
    /// Physical table location (e.g., S3 URI).
    pub location: Option<String>,
    /// Table properties from Iceberg metadata.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub properties: HashMap<String, String>,
}

/// A partition field from an Iceberg partition spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionField {
    /// Source column name (resolved from field ID).
    pub source_column: String,
    /// Partition transform applied to the source column.
    pub transform: PartitionTransform,
    /// Partition field name in the spec.
    pub name: String,
    /// Inferred temporal grain from the transform (if temporal).
    pub inferred_grain: Option<TemporalGrain>,
}

/// Iceberg partition transforms.
///
/// Temporal transforms (`Year`, `Month`, `Day`, `Hour`) map directly to
/// `TemporalGrain` for auto-inferring native grain on datasets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PartitionTransform {
    Identity,
    Year,
    Month,
    Day,
    Hour,
    Bucket { n: u32 },
    Truncate { width: u32 },
}

impl PartitionTransform {
    /// Infer the temporal grain from this partition transform.
    ///
    /// Returns `Some(grain)` for temporal transforms, `None` for others.
    pub fn inferred_grain(&self) -> Option<TemporalGrain> {
        match self {
            PartitionTransform::Year => Some(TemporalGrain::Year),
            PartitionTransform::Month => Some(TemporalGrain::Month),
            PartitionTransform::Day => Some(TemporalGrain::Day),
            PartitionTransform::Hour => Some(TemporalGrain::Hour),
            _ => None,
        }
    }

    /// Parse an Iceberg transform string into a `PartitionTransform`.
    ///
    /// Handles: "identity", "year", "month", "day", "hour", "bucket[N]", "truncate[N]", "void".
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "identity" => Some(PartitionTransform::Identity),
            "year" => Some(PartitionTransform::Year),
            "month" => Some(PartitionTransform::Month),
            "day" => Some(PartitionTransform::Day),
            "hour" => Some(PartitionTransform::Hour),
            "void" => None,
            s if s.starts_with("bucket[") && s.ends_with(']') => {
                let n: u32 = s[7..s.len() - 1].parse().ok()?;
                Some(PartitionTransform::Bucket { n })
            }
            s if s.starts_with("truncate[") && s.ends_with(']') => {
                let width: u32 = s[9..s.len() - 1].parse().ok()?;
                Some(PartitionTransform::Truncate { width })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_partition_transform_inferred_grain() {
        assert_eq!(PartitionTransform::Year.inferred_grain(), Some(TemporalGrain::Year));
        assert_eq!(PartitionTransform::Month.inferred_grain(), Some(TemporalGrain::Month));
        assert_eq!(PartitionTransform::Day.inferred_grain(), Some(TemporalGrain::Day));
        assert_eq!(PartitionTransform::Hour.inferred_grain(), Some(TemporalGrain::Hour));
        assert_eq!(PartitionTransform::Identity.inferred_grain(), None);
        assert_eq!((PartitionTransform::Bucket { n: 16 }).inferred_grain(), None);
        assert_eq!((PartitionTransform::Truncate { width: 10 }).inferred_grain(), None);
    }

    #[test]
    fn test_partition_transform_parse() {
        assert_eq!(PartitionTransform::parse("identity"), Some(PartitionTransform::Identity));
        assert_eq!(PartitionTransform::parse("year"), Some(PartitionTransform::Year));
        assert_eq!(PartitionTransform::parse("month"), Some(PartitionTransform::Month));
        assert_eq!(PartitionTransform::parse("day"), Some(PartitionTransform::Day));
        assert_eq!(PartitionTransform::parse("hour"), Some(PartitionTransform::Hour));
        assert_eq!(PartitionTransform::parse("bucket[16]"), Some(PartitionTransform::Bucket { n: 16 }));
        assert_eq!(PartitionTransform::parse("truncate[10]"), Some(PartitionTransform::Truncate { width: 10 }));
        assert_eq!(PartitionTransform::parse("void"), None);
        assert_eq!(PartitionTransform::parse("unknown"), None);
    }

    #[test]
    fn test_catalog_snapshot_serde_roundtrip() {
        let snapshot = CatalogSnapshot {
            tables: {
                let mut m = HashMap::new();
                m.insert(
                    "default.orders".to_string(),
                    TableSnapshot {
                        fqn: "default.orders".to_string(),
                        columns: vec![
                            ResolvedColumn {
                                name: "id".to_string(),
                                data_type: DataType::Int64,
                                nullable: false,
                                comment: None,
                                field_id: Some(1),
                            },
                            ResolvedColumn {
                                name: "amount".to_string(),
                                data_type: DataType::Float64,
                                nullable: true,
                                comment: Some("Order amount".to_string()),
                                field_id: Some(2),
                            },
                        ],
                        iceberg: Some(IcebergMetadata {
                            snapshot_id: 123456789,
                            partition_spec: vec![PartitionField {
                                source_column: "order_date".to_string(),
                                transform: PartitionTransform::Day,
                                name: "order_date_day".to_string(),
                                inferred_grain: Some(TemporalGrain::Day),
                            }],
                            format_version: Some(2),
                            location: Some("s3://bucket/warehouse/orders".to_string()),
                            properties: HashMap::new(),
                        }),
                    },
                );
                m
            },
            captured_at: chrono::Utc::now(),
        };

        let json = serde_json::to_string(&snapshot).expect("serialize");
        let deserialized: CatalogSnapshot = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deserialized.tables.len(), 1);
        let table = deserialized.tables.get("default.orders").unwrap();
        assert_eq!(table.columns.len(), 2);
        assert_eq!(table.columns[0].name, "id");
        let iceberg = table.iceberg.as_ref().unwrap();
        assert_eq!(iceberg.snapshot_id, 123456789);
        assert_eq!(iceberg.partition_spec.len(), 1);
        assert_eq!(iceberg.partition_spec[0].transform, PartitionTransform::Day);
        assert_eq!(iceberg.partition_spec[0].inferred_grain, Some(TemporalGrain::Day));
    }
}
