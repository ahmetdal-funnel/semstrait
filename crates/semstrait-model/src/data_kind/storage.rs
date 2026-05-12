//! `StorageConfig`, `StorageFormat`, `PartitionDef`, `CatalogRef` — `32 §4`.

use bon::Builder;
use serde::{Deserialize, Serialize};

/// Catalog reference. Bare alias string at the YAML surface (`32b §4`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[non_exhaustive]
pub struct CatalogRef {
    /// Alias — keys a `CatalogEntry` in `catalogs.yaml`.
    pub alias: String,
}

impl CatalogRef {
    pub fn new(alias: impl Into<String>) -> Self {
        Self {
            alias: alias.into(),
        }
    }
}

/// Storage format roster for path-based sources. Required when `paths`
/// is non-empty per `32 §4`. Catalog-resolved sources omit format —
/// the catalog metadata supplies it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum StorageFormat {
    Parquet,
    Csv,
    Json,
    Orc,
    Avro,
}

/// `extras.storage:` block — file / folder / glob URIs and / or
/// catalog FQNs / table-name globs, plus an optional declared
/// partition layout.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Builder)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
#[builder(start_fn = builder, finish_fn = build)]
pub struct StorageConfig {
    /// Storage format for path-based sources. Required when `paths`
    /// is non-empty. Ignored when only `tables` is authored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<StorageFormat>,

    /// File / folder / glob URIs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[builder(default)]
    pub paths: Vec<String>,

    /// Catalog FQNs or table-name globs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[builder(default)]
    pub tables: Vec<String>,

    /// Catalog-less partition declaration for file sources. v1
    /// runtime-dormant; carried through compile for v2+ partition-aware
    /// planning per `32 §4`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partition_def: Option<PartitionDef>,
}

impl StorageConfig {
    pub fn is_empty(&self) -> bool {
        self.format.is_none()
            && self.paths.is_empty()
            && self.tables.is_empty()
            && self.partition_def.is_none()
    }
}

/// Author-declared partition layout for file sources. Authoring
/// surface only; the v1 runtime defers partition pruning to
/// engine-side discovery from filter predicates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub enum PartitionDef {
    /// Range-partitioned by a single column (e.g. `order_date`).
    Range {
        column: String,
    },
    /// List-partitioned by enumerated values on a single column.
    List {
        column: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        values: Vec<String>,
    },
}
