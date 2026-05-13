//! `DataKindBase<E>`, `LeafExtras`, `ComplexExtras` — `32 §3.1`, `§4`.

use crate::data_kind::storage::{CatalogRef, PartitionDef, StorageConfig, StorageFormat};
use crate::entities::mapping::SemanticMapping;
use crate::entities::temporal::TemporalShape;
use bon::Builder;
use serde::{Deserialize, Serialize};

/// Common-fields struct held inside every per-variant body. Carries
/// the universal `name` and the per-axis `extras` flavor.
///
/// At the YAML surface the fields are flattened directly onto the
/// containing body so authors write `name:` and `extras:` at the
/// data-kind top level (no `base:` wrapper).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct DataKindBase<E: ExtrasFlavor> {
    pub name: String,

    #[serde(default = "<E as Default>::default", skip_serializing_if = "ExtrasFlavor::is_default")]
    pub extras: E,
}

/// Helper trait — lets `DataKindBase<E>` carry a `Default`-able extras
/// flavor without requiring callers to author `extras: {}` in YAML.
pub trait ExtrasFlavor: Default + PartialEq {
    fn is_default(&self) -> bool;
}

/// Leaf-axis extras — `Dataset` / `NestedDataset`. Carries the full
/// physical-source surface plus the optional temporal shape.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Builder)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
#[builder(start_fn = builder, finish_fn = build)]
pub struct LeafExtras {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog: Option<CatalogRef>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<StorageConfig>,

    /// `semantic_mapping:` — implicit `Auto` when omitted (per
    /// `18 §10.3` / `32 §5.1`).
    #[serde(default, skip_serializing_if = "is_default_mapping")]
    #[builder(default)]
    pub semantic_mapping: SemanticMapping,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temporal: Option<TemporalShape>,
}

fn is_default_mapping(m: &SemanticMapping) -> bool {
    m.is_auto()
}

impl ExtrasFlavor for LeafExtras {
    fn is_default(&self) -> bool {
        self.catalog.is_none()
            && self.storage.is_none()
            && self.semantic_mapping.is_auto()
            && self.temporal.is_none()
    }
}

// Inherent `with_*` setters — single source of truth for the LeafExtras
// flattener logic that `DatasetBuilder` / `NestedDatasetBuilder` facade
// methods (`32 §9.7.8.5`) delegate to. Storage sub-fields use
// `Option::get_or_insert_with(Default::default)` so the storage block is
// auto-created on first sub-field set without an explicit `.storage(...)`
// call.
impl LeafExtras {
    pub fn with_catalog(mut self, c: impl Into<String>) -> Self {
        self.catalog = Some(CatalogRef::new(c));
        self
    }
    pub fn with_storage(mut self, s: StorageConfig) -> Self {
        self.storage = Some(s);
        self
    }
    pub fn with_format(mut self, f: StorageFormat) -> Self {
        self.storage.get_or_insert_with(Default::default).format = Some(f);
        self
    }
    pub fn with_path(mut self, p: impl Into<String>) -> Self {
        self.storage.get_or_insert_with(Default::default).paths.push(p.into());
        self
    }
    pub fn with_paths(mut self, items: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.storage
            .get_or_insert_with(Default::default)
            .paths
            .extend(items.into_iter().map(Into::into));
        self
    }
    pub fn with_table(mut self, t: impl Into<String>) -> Self {
        self.storage.get_or_insert_with(Default::default).tables.push(t.into());
        self
    }
    pub fn with_tables(mut self, items: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.storage
            .get_or_insert_with(Default::default)
            .tables
            .extend(items.into_iter().map(Into::into));
        self
    }
    pub fn with_partition_def(mut self, p: PartitionDef) -> Self {
        self.storage.get_or_insert_with(Default::default).partition_def = Some(p);
        self
    }
    pub fn with_semantic_mapping(mut self, m: SemanticMapping) -> Self {
        self.semantic_mapping = m;
        self
    }
    pub fn with_temporal(mut self, t: TemporalShape) -> Self {
        self.temporal = Some(t);
        self
    }
}

/// Complex-axis extras — `Grainset` / `Unionset` / `Joinset` plus
/// their nested forms. Per R-6, `catalog` / `storage` /
/// `semantic_mapping` are leaf-only and have no slot here (SR-5).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Builder)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
#[builder(start_fn = builder, finish_fn = build)]
pub struct ComplexExtras {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temporal: Option<TemporalShape>,
}

impl ExtrasFlavor for ComplexExtras {
    fn is_default(&self) -> bool {
        self.temporal.is_none()
    }
}

// Inherent `with_*` setters — single source of truth for the
// ComplexExtras flattener logic that complex DataKind builder facades
// (`32 §9.7.8.5`) delegate to.
impl ComplexExtras {
    pub fn with_temporal(mut self, t: TemporalShape) -> Self {
        self.temporal = Some(t);
        self
    }
}
