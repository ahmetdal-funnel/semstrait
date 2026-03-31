//! Compiled manifest types — the output of the compilation pipeline.
//!
//! All types are `Serialize + Deserialize` for JSON persistence.
//! No `GlobPattern` survives into these types — all datasets are fully expanded.

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use semstrait_core::expr::Aggregation;
use semstrait_core::{DataType, Expr};
use semstrait_model::{
    AdditivityType, DimensionType, JoinColumnPair, JoinType, Cardinality,
    MeasureConstraints, MeasureFilter,
};
use serde::{Deserialize, Serialize};

// ============================================================================
// CompiledManifest
// ============================================================================

/// The versioned, JSON-serializable output of `ManifestCompiler::compile()`.
///
/// v3 schema: single `data_kinds` map containing all compiled entities
/// (datasets, grainsets, unionsets, joinsets) with acceleration structures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledManifest {
    /// Schema version for forward compatibility.
    pub version: u32,
    /// Timestamp of compilation.
    pub compiled_at: DateTime<Utc>,
    /// SHA-256 hash of the YAML source input.
    pub source_hash: String,
    /// All queryable semantic entities with acceleration structures.
    pub data_kinds: IndexMap<String, crate::acceleration::CompiledDataKind>,
    /// Top-level compiled relationships.
    pub relationships: Vec<CompiledRelationship>,
    /// Global relationship graph for ad-hoc join resolution.
    #[serde(default)]
    pub relationship_graph: crate::acceleration::RelationshipGraph,
    /// Global field index for ad-hoc join resolution.
    #[serde(default)]
    pub field_index: crate::acceleration::FieldIndex,
    /// Unified semantic graph (petgraph-based). Combines relationship traversal
    /// and field indexing into a single structure.
    #[serde(skip)]
    pub semantic_graph: crate::acceleration::SemanticGraph,
    /// Compilation diagnostics (warnings, info).
    #[serde(default)]
    pub diagnostics: crate::acceleration::CompileDiagnostics,
    /// Catalog metadata snapshot captured at compile time (steps 10-13).
    /// Present when a CatalogProvider was available during compilation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog_snapshot: Option<crate::catalog_snapshot::CatalogSnapshot>,
    /// Model name.
    pub model_name: String,
    /// Model description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_description: Option<String>,
}

// ============================================================================
// CompiledDataset
// ============================================================================

// ============================================================================
// CompiledDimension
// ============================================================================

/// A compiled dimension definition.
///
/// When `expr` is `Some`, this is a **computed dimension** — derived from an
/// expression tree rather than a physical column. The planner emits it as a
/// ProjectNode expression instead of a ScanNode column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledDimension {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub data_type: DataType,
    pub dim_type: DimensionType,
    /// Compiled expression tree — `None` for regular (physical) dimensions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expr: Option<semstrait_core::Expr>,
    /// Original YAML expression source for debugging/display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expr_source: Option<String>,
}

// ============================================================================
// CompiledMeasure
// ============================================================================

/// A compiled measure with parsed Expr and resolved aggregation.
///
/// Every measure declares an aggregation function (`agg`). The `expr` field
/// contains a horizontal-only expression (no aggregation functions) that is
/// wrapped by `agg` during planning. When `expr` references a single column
/// matching the measure name, the column is resolved from `column_mapping`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledMeasure {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub data_type: DataType,
    /// Aggregation function applied to `expr`. Always present — the compiler
    /// derives it from the declarative `agg:` tag or auto-upgrades legacy
    /// expressions during compilation.
    #[serde(deserialize_with = "deserialize_aggregation")]
    pub agg: Aggregation,
    /// Parsed horizontal DSL expression tree (no aggregation functions).
    /// Wrapped by `agg` during plan generation.
    pub expr: Expr,
    /// Original expression string for debugging.
    pub expr_source: String,
    /// Additivity classification. Compiler-derived from `agg` when not
    /// explicitly specified in YAML (SUM/COUNT/MIN/MAX → Full, AVG/CountDistinct → Non).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additivity: Option<AdditivityType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraints: Option<MeasureConstraints>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<CompiledFilter>,
}

// ============================================================================
// MetricType
// ============================================================================

/// Compiler-inferred metric category. Determines planning strategy:
/// - `Simple`: wraps exactly one measure (can be inlined)
/// - `Ratio`: numerator / denominator — components aggregated separately, divided in post-agg layer
/// - `Derived`: arbitrary formula over measures/metrics
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricType {
    /// Wraps a single measure reference (e.g., `expr: "revenue"`).
    Simple,
    /// Top-level division of two measure references (e.g., `expr: "clicks / impressions"`).
    /// Components MUST be aggregated separately; the ratio is computed post-aggregation.
    Ratio,
    /// Arbitrary arithmetic/logic over measures and/or other metrics.
    Derived,
}

// ============================================================================
// CompiledMetric
// ============================================================================

/// A compiled metric with parsed Expr.
///
/// Metrics compose already-aggregated measures in the post-aggregate layer.
/// `metric_type` is compiler-inferred from the expression tree structure.
/// `agg` is optional — when present, creates a two-stage plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledMetric {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub data_type: DataType,
    /// Compiler-inferred metric category (Simple, Ratio, Derived).
    #[serde(default = "default_metric_type")]
    pub metric_type: MetricType,
    /// Declarative aggregation for two-stage metric computation.
    /// When present, the metric creates a two-stage plan (inner + outer
    /// aggregation). When absent, the metric is a pure derived expression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agg: Option<Aggregation>,
    /// Parsed DSL expression tree.
    pub expr: Expr,
    /// Original expression string.
    pub expr_source: String,
    /// Effective additivity derived from transitive leaf measures at compile time.
    /// Worst-case of all leaf measure additivity values (Full < Semi < Non).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additivity: Option<AdditivityType>,
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
            expr_source: mf.expr.display_string(),
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

impl CompiledManifest {
    /// Resolve a queryable entity by name from the `data_kinds` map.
    ///
    /// This is the primary resolution method. Returns `None` if not found.
    pub fn resolve(&self, name: &str) -> Option<&crate::acceleration::CompiledDataKind> {
        self.data_kinds.get(name)
    }
}

// ============================================================================
// Serde helpers
// ============================================================================

/// Deserialize `Aggregation` with backward compatibility for existing JSON
/// manifests where `agg` was `Option<Aggregation>`. Treats `null` or missing
/// value as `Aggregation::Sum` (the most common default).
fn deserialize_aggregation<'de, D>(deserializer: D) -> Result<Aggregation, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt: Option<Aggregation> = Option::deserialize(deserializer)?;
    Ok(opt.unwrap_or(Aggregation::Sum))
}

/// Default `MetricType` for serde backward compatibility with manifests
/// compiled before MetricType was added.
fn default_metric_type() -> MetricType {
    MetricType::Derived
}

// ============================================================================
// MetricType helpers
// ============================================================================

impl MetricType {
    /// Infer `MetricType` from a compiled metric expression tree.
    ///
    /// - Single `EntityRef` or `Column` → `Simple`
    /// - Top-level `BinaryOp(_, Divide|SafeDivide, _)` where both sides are
    ///   leaf measure/metric references → `Ratio`
    /// - Everything else → `Derived`
    pub fn infer(expr: &Expr) -> Self {
        match expr {
            Expr::EntityRef(_) | Expr::Column(_) => MetricType::Simple,
            Expr::BinaryOp(bin) => {
                use semstrait_core::expr::BinaryOp;
                if matches!(bin.op, BinaryOp::Divide | BinaryOp::SafeDivide)
                    && is_leaf_ref(&bin.left)
                    && is_leaf_ref(&bin.right)
                {
                    MetricType::Ratio
                } else {
                    MetricType::Derived
                }
            }
            _ => MetricType::Derived,
        }
    }
}

/// Check if an expression is a leaf reference (EntityRef or Column).
fn is_leaf_ref(expr: &Expr) -> bool {
    matches!(expr, Expr::EntityRef(_) | Expr::Column(_))
}

// ============================================================================
// Additivity derivation
// ============================================================================

/// Derive `AdditivityType` from an `Aggregation` function.
///
/// This is the default when the user doesn't explicitly specify `additivity:`
/// in YAML. SUM, COUNT, MIN, MAX are fully additive. AVG and COUNT_DISTINCT
/// are non-additive (cannot be safely re-aggregated across datasets).
pub fn derive_additivity(agg: Aggregation) -> AdditivityType {
    match agg {
        Aggregation::Sum | Aggregation::Count | Aggregation::Min | Aggregation::Max => {
            AdditivityType::Full
        }
        Aggregation::Avg | Aggregation::CountDistinct => AdditivityType::Non,
    }
}

/// Compute the worst-case (effective) additivity from a set of leaf measure
/// additivity values. Ordering: Full < Semi < Non.
///
/// When two Semi values have different non-additive dimensions, the result is
/// Non (cannot safely re-aggregate along either dimension set).
pub fn worst_case_additivity<'a>(
    additivity_values: impl Iterator<Item = &'a AdditivityType>,
) -> AdditivityType {
    let mut worst = AdditivityType::Full;
    for a in additivity_values {
        worst = match (&worst, a) {
            (_, AdditivityType::Non) | (AdditivityType::Non, _) => AdditivityType::Non,
            (AdditivityType::Semi(existing), AdditivityType::Semi(new)) => {
                // Two Semi values: if they have different non-additive dimensions,
                // escalate to Non (cannot safely re-aggregate either).
                if existing.non_additive_dimensions == new.non_additive_dimensions {
                    worst // Same dimensions — keep existing Semi
                } else {
                    AdditivityType::Non
                }
            }
            (_, AdditivityType::Semi(s)) => AdditivityType::Semi(s.clone()),
            (AdditivityType::Semi(_), _) => worst,
            _ => AdditivityType::Full,
        };
        if matches!(worst, AdditivityType::Non) {
            break; // Can't get worse
        }
    }
    worst
}
