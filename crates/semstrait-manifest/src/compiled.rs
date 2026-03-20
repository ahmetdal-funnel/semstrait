//! Compiled manifest types — the output of the compilation pipeline.
//!
//! All types are `Serialize + Deserialize` for JSON persistence.
//! No `GlobPattern` survives into these types — all datasets are fully expanded.

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use semstrait_core::Expr;
use semstrait_model::{
    Additivity, AdditivityType, Cardinality, ColumnMapping, ColumnMappingValue, DimensionType,
    JoinAssociativity, JoinColumnPair, JoinType, Keys, KindDatasetExtras, KindTypeSpec,
    MeasureConstraints, MeasureFilter, TemporalGrain, UnionMode,
};
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

// ============================================================================
// CompiledManifest
// ============================================================================

/// The versioned, JSON-serializable output of `ManifestCompiler::compile()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledManifest {
    /// Schema version for forward compatibility.
    pub version: u32,
    /// Timestamp of compilation.
    pub compiled_at: DateTime<Utc>,
    /// SHA-256 hash of the YAML source input.
    pub source_hash: String,
    /// Compiled datasets, keyed by name.
    pub datasets: IndexMap<String, CompiledDataset>,
    /// Compiled kinds, keyed by name.
    pub kinds: IndexMap<String, CompiledKind>,
    /// Top-level compiled relationships.
    pub relationships: Vec<CompiledRelationship>,
    /// Model name.
    pub model_name: String,
    /// Model description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_description: Option<String>,
}

// ============================================================================
// CompiledDataset
// ============================================================================

/// A fully compiled dataset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledDataset {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keys: Option<Keys>,
    pub dimensions: IndexMap<String, CompiledDimension>,
    pub measures: IndexMap<String, CompiledMeasure>,
    pub metrics: IndexMap<String, CompiledMetric>,
    /// Schema snapshot captured at compile time (when catalog was available).
    /// Used for drift detection (PLAN_W003).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compiled_schema: Option<Vec<SchemaColumn>>,
}

/// A column in the compiled schema snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchemaColumn {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

// ============================================================================
// CompiledKind
// ============================================================================

/// A fully compiled kind with interface, strategy, and binding layers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledKind {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    // Interface layer
    pub dimensions: IndexMap<String, CompiledDimension>,
    pub measures: IndexMap<String, CompiledMeasure>,
    pub metrics: IndexMap<String, CompiledMetric>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keys: Option<Keys>,

    // Strategy layer
    pub kind_type: CompiledKindType,

    // Binding layer
    pub datasets: Vec<CompiledKindDataset>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<CompiledRelationship>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<Vec<String>>,
    /// Kind-level filters applied to all queries against this kind.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<CompiledFilter>,
}

// ============================================================================
// CompiledKindType
// ============================================================================

/// Compiled kind type (mirrors KindTypeSpec but serializable).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CompiledKindType {
    Grainset,
    Unionset {
        #[serde(default)]
        union_mode: UnionMode,
    },
    Joinset {
        associativity: JoinAssociativity,
    },
}

impl From<&KindTypeSpec> for CompiledKindType {
    fn from(spec: &KindTypeSpec) -> Self {
        match spec {
            KindTypeSpec::Grainset => CompiledKindType::Grainset,
            KindTypeSpec::Unionset(config) => CompiledKindType::Unionset {
                union_mode: config.union_mode,
            },
            KindTypeSpec::Joinset(config) => CompiledKindType::Joinset {
                associativity: config.associativity,
            },
        }
    }
}

// ============================================================================
// CompiledKindDataset
// ============================================================================

/// A fully expanded dataset binding within a kind.
/// No GlobPattern — all datasets are concrete.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledKindDataset {
    /// Concrete dataset name (never a glob).
    pub name: String,
    /// Column mapping extras.
    pub extras: KindDatasetExtras,
}

// ============================================================================
// CompiledDimension
// ============================================================================

/// A compiled dimension definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledDimension {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub data_type: String,
    pub dim_type: DimensionType,
}

// ============================================================================
// CompiledMeasure
// ============================================================================

/// A compiled measure with parsed Expr.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledMeasure {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub data_type: String,
    /// Parsed DSL expression tree (never a raw string).
    pub expr: Expr,
    /// Original expression string for debugging.
    pub expr_source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additivity: Option<Additivity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraints: Option<MeasureConstraints>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<CompiledFilter>,
}

// ============================================================================
// CompiledMetric
// ============================================================================

/// A compiled metric with parsed Expr.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledMetric {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub data_type: String,
    /// Parsed DSL expression tree.
    pub expr: Expr,
    /// Original expression string.
    pub expr_source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additivity: Option<Additivity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraints: Option<MeasureConstraints>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<CompiledFilter>,
    /// Dependency depth in the metric graph.
    pub depth: usize,
}

// ============================================================================
// CompiledFilter
// ============================================================================

/// A compiled filter with parsed Expr.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledFilter {
    pub name: String,
    pub expr: Expr,
    pub expr_source: String,
}

impl CompiledFilter {
    pub fn from_measure_filter(mf: &MeasureFilter, parsed_expr: Expr) -> Self {
        Self {
            name: mf.name.clone(),
            expr: parsed_expr,
            expr_source: mf.expr.clone(),
        }
    }
}

// ============================================================================
// CompiledRelationship
// ============================================================================

/// A compiled relationship between datasets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledRelationship {
    pub name: String,
    pub from: String,
    pub to: String,
    pub join_type: JoinType,
    pub columns: Vec<JoinColumnPair>,
    pub cardinality: Cardinality,
}

// ============================================================================
// Convenience accessors
// ============================================================================

/// A queryable entity — either an explicit kind or a dataset auto-wrapped as an implicit kind.
pub enum QueryableEntity<'a> {
    /// An explicitly defined kind in the manifest.
    Kind(&'a CompiledKind),
    /// A dataset auto-synthesized into an implicit grainset kind.
    ImplicitKind(Box<CompiledKind>),
}

impl<'a> QueryableEntity<'a> {
    /// Get a reference to the underlying `CompiledKind`.
    pub fn as_kind(&self) -> &CompiledKind {
        match self {
            QueryableEntity::Kind(k) => k,
            QueryableEntity::ImplicitKind(k) => k,
        }
    }
}

impl CompiledManifest {
    /// Look up a kind by name.
    pub fn get_kind(&self, name: &str) -> Option<&CompiledKind> {
        self.kinds.get(name)
    }

    /// Look up a dataset by name.
    pub fn get_dataset(&self, name: &str) -> Option<&CompiledDataset> {
        self.datasets.get(name)
    }

    /// Resolve a query entity by name: checks kinds first, then datasets.
    ///
    /// If the name matches a kind, returns `Kind(...)`.
    /// If the name matches a dataset, auto-wraps it as an implicit grainset kind.
    /// Returns `None` if neither exists.
    pub fn resolve_entity(&self, name: &str) -> Option<QueryableEntity<'_>> {
        // Kinds take precedence (name uniqueness is enforced at compile time).
        if let Some(kind) = self.kinds.get(name) {
            return Some(QueryableEntity::Kind(kind));
        }

        // Auto-synthesize dataset as an implicit grainset kind.
        if let Some(dataset) = self.datasets.get(name) {
            return Some(QueryableEntity::ImplicitKind(
                Box::new(dataset_to_implicit_kind(dataset)),
            ));
        }

        None
    }
}

/// Synthesize a `CompiledKind` from a `CompiledDataset` by wrapping it
/// as a single-dataset grainset with identity column mapping.
fn dataset_to_implicit_kind(ds: &CompiledDataset) -> CompiledKind {
    // Build identity column mapping from the dataset's interface names.
    let interface_names: Vec<&String> = ds
        .dimensions
        .keys()
        .chain(ds.measures.keys())
        .chain(ds.metrics.keys())
        .collect();

    let mapping: HashMap<String, ColumnMappingValue> = interface_names
        .iter()
        .map(|name| ((*name).clone(), ColumnMappingValue::Simple((*name).clone())))
        .collect();

    CompiledKind {
        name: ds.name.clone(),
        description: ds.description.clone(),
        dimensions: ds.dimensions.clone(),
        measures: ds.measures.clone(),
        metrics: ds.metrics.clone(),
        keys: ds.keys.clone(),
        kind_type: CompiledKindType::Grainset,
        datasets: vec![CompiledKindDataset {
            name: ds.name.clone(),
            extras: KindDatasetExtras {
                column_mapping: ColumnMapping::Explicit(mapping),
                temporal: None,
                storage: None,
                catalog: None,
            },
        }],
        relationships: vec![],
        domain: ds.domain.clone(),
        filters: vec![],
    }
}

impl CompiledKind {
    /// Returns the grain entries for a specific temporal dimension, if present.
    pub fn temporal_grains(&self, dim_name: &str) -> Option<Vec<TemporalGrain>> {
        let dim = self.dimensions.get(dim_name)?;
        match &dim.dim_type {
            DimensionType::Temporal(t) => Some(t.grains.clone()),
            _ => None,
        }
    }

    /// Returns true if this kind has any additivity type other than Full.
    pub fn has_non_full_additivity(&self) -> bool {
        self.measures.values().any(|m| {
            m.additivity.as_ref().is_some_and(|a| {
                !matches!(a.additivity_type, AdditivityType::Full)
            })
        })
    }
}
