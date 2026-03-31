//! Storage and catalog types for semantic models.

use semstrait_core::DataFormat;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Reference to a named catalog from `catalogs.yaml`.
///
/// Supports two YAML forms:
/// - Shorthand: `catalog: polaris_prod` (alias only)
/// - Struct: `catalog: { alias: polaris_prod, namespace: tiktok }`
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CatalogRef {
    /// Alias referencing a catalog entry from `catalogs.yaml`.
    pub alias: String,
    /// Override the namespace for this entity (optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

impl<'de> Deserialize<'de> for CatalogRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de;

        struct CatalogRefVisitor;

        impl<'de> de::Visitor<'de> for CatalogRefVisitor {
            type Value = CatalogRef;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a catalog alias string or a struct with `alias` and optional `namespace`")
            }

            fn visit_str<E>(self, value: &str) -> Result<CatalogRef, E>
            where
                E: de::Error,
            {
                Ok(CatalogRef {
                    alias: value.to_string(),
                    namespace: None,
                })
            }

            fn visit_map<A>(self, map: A) -> Result<CatalogRef, A::Error>
            where
                A: de::MapAccess<'de>,
            {
                #[derive(Deserialize)]
                struct CatalogRefFields {
                    alias: String,
                    #[serde(default)]
                    namespace: Option<String>,
                }
                let fields = CatalogRefFields::deserialize(
                    de::value::MapAccessDeserializer::new(map),
                )?;
                Ok(CatalogRef {
                    alias: fields.alias,
                    namespace: fields.namespace,
                })
            }
        }

        deserializer.deserialize_any(CatalogRefVisitor)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorageConfig {
    /// Data format. Required for paths (parquet/csv/iceberg), omitted for tables (catalog knows).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<DataFormat>,
    /// File/object store paths (local://, s3://). May contain globs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
    /// Catalog table references (namespace.table). May contain wildcards.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tables: Vec<String>,
    /// Partition definition for metadata dimensions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partition_def: Option<PartitionDef>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PartitionDef {
    #[serde(rename = "type")]
    pub partition_type: PartitionType,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PartitionType {
    Range(RangePartition),
    List(ListPartition),
}

impl<'de> Deserialize<'de> for PartitionType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let map: HashMap<String, serde_yaml::Value> = Deserialize::deserialize(deserializer)?;
        if map.len() != 1 {
            return Err(serde::de::Error::custom(
                "PartitionType must have exactly one variant key",
            ));
        }
        let (key, value) = map.into_iter().next()
            .ok_or_else(|| serde::de::Error::custom("expected one variant key"))?;
        match key.as_str() {
            "range" => Ok(PartitionType::Range(
                serde_yaml::from_value(value).map_err(serde::de::Error::custom)?,
            )),
            "list" => Ok(PartitionType::List(
                serde_yaml::from_value(value).map_err(serde::de::Error::custom)?,
            )),
            other => Err(serde::de::Error::unknown_variant(other, &["range", "list"])),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RangePartition {
    pub column: String,
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListPartition {
    pub column: String,
    pub values: Vec<String>,
}
