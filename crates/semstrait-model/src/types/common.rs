//! Common types shared across the semantic model.

use crate::expr_block::ExprSource;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::str::FromStr;

use super::dimension::DimensionEntry;
use super::keys::Keys;
use super::measure::MeasureEntry;
use super::metric::MetricEntry;
use super::relationship::Relationship;
#[allow(unused_imports)]
use super::temporal::TemporalGrain;

// =============================================================================
// Model-level DataType (YAML-facing, simple type names)
// =============================================================================

/// Data types supported in semantic model YAML definitions.
///
/// These are simple, user-facing type names that map to the YAML `data_type` field.
/// They are distinct from the Arrow-aligned `semstrait_core::DataType` used in IR plans.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum DataType {
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
    Bool,
    #[default]
    String,
    Date,
    Timestamp,
    Decimal { precision: u8, scale: u8 },
}

impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataType::I8 => write!(f, "i8"),
            DataType::I16 => write!(f, "i16"),
            DataType::I32 => write!(f, "i32"),
            DataType::I64 => write!(f, "i64"),
            DataType::F32 => write!(f, "f32"),
            DataType::F64 => write!(f, "f64"),
            DataType::Bool => write!(f, "bool"),
            DataType::String => write!(f, "string"),
            DataType::Date => write!(f, "date"),
            DataType::Timestamp => write!(f, "timestamp"),
            DataType::Decimal { precision, scale } => write!(f, "decimal({}, {})", precision, scale),
        }
    }
}

impl FromStr for DataType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lower = s.to_lowercase();
        if lower.starts_with("decimal(") && lower.ends_with(')') {
            let inner = &lower[8..lower.len() - 1];
            let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
            if parts.len() != 2 {
                return Err(format!("invalid decimal type: '{}'", s));
            }
            let precision: u8 = parts[0].parse().map_err(|_| format!("invalid precision in '{}'", s))?;
            let scale: u8 = parts[1].parse().map_err(|_| format!("invalid scale in '{}'", s))?;
            return Ok(DataType::Decimal { precision, scale });
        }
        match lower.as_str() {
            "i8" => Ok(DataType::I8),
            "i16" => Ok(DataType::I16),
            "i32" | "int" | "integer" => Ok(DataType::I32),
            "i64" | "long" | "bigint" | "int64" => Ok(DataType::I64),
            "f32" | "float" | "float32" => Ok(DataType::F32),
            "f64" | "double" | "float64" => Ok(DataType::F64),
            "bool" | "boolean" => Ok(DataType::Bool),
            "string" | "text" | "varchar" => Ok(DataType::String),
            "date" => Ok(DataType::Date),
            "timestamp" | "datetime" => Ok(DataType::Timestamp),
            "decimal" => Ok(DataType::Decimal { precision: 18, scale: 2 }),
            _ => Err(format!("unknown data type: '{}'", s)),
        }
    }
}

impl<'de> Deserialize<'de> for DataType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = std::string::String::deserialize(deserializer)?;
        DataType::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl Serialize for DataType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

// =============================================================================
// AI Context
// =============================================================================

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AiContext {
    #[serde(default)]
    pub synonyms: Option<Vec<String>>,
    #[serde(default)]
    pub query_patterns: Option<Vec<String>>,
    #[serde(default)]
    pub value_examples: Option<Vec<String>>,
    #[serde(default)]
    pub semantic_tags: Option<Vec<String>>,
    #[serde(default)]
    pub aggregation_intent: Option<String>,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default, rename = "not")]
    pub not_: Option<Vec<String>>,
    #[serde(default)]
    pub entity: Option<String>,
    #[serde(default)]
    pub importance: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub authoritative_for: Option<Vec<String>>,
    #[serde(default)]
    pub not_for: Option<Vec<String>>,
}

// =============================================================================
// Column Mapping
// =============================================================================

/// Column mapping: either `auto`, `inherited`, or an explicit map.
#[derive(Debug, Clone)]
pub enum ColumnMapping {
    /// Auto-map: all kind interface names are matched 1:1 to physical columns.
    /// Expanded to `Explicit` identity mapping during compilation (step 4.5).
    Auto,
    /// Inherit from kind.extras.column_mapping. Resolved in step 4.5.
    /// This is the default when `column_mapping:` is absent from a dataset's extras.
    Inherited,
    /// Explicit mapping of semantic name → physical column.
    Explicit(HashMap<String, ColumnMappingValue>),
}

impl ColumnMapping {
    /// Create an explicit column mapping.
    pub fn explicit(map: HashMap<String, ColumnMappingValue>) -> Self {
        ColumnMapping::Explicit(map)
    }

    /// Returns true if this is `Auto`.
    pub fn is_auto(&self) -> bool {
        matches!(self, ColumnMapping::Auto)
    }

    /// Returns true if this is `Inherited`.
    pub fn is_inherited(&self) -> bool {
        matches!(self, ColumnMapping::Inherited)
    }

    /// Default value for `InlineDatasetExtras.column_mapping` when the field is absent.
    /// Used by `#[serde(default = "ColumnMapping::default_inherited")]`.
    pub fn default_inherited() -> Self {
        ColumnMapping::Inherited
    }

    /// Get the underlying map. Panics if `Auto` or `Inherited` (must be expanded before use).
    pub fn as_map(&self) -> &HashMap<String, ColumnMappingValue> {
        match self {
            ColumnMapping::Explicit(m) => m,
            ColumnMapping::Auto | ColumnMapping::Inherited => {
                panic!("column_mapping must be expanded before use (call expand_auto_mappings first)")
            }
        }
    }
}

impl From<HashMap<String, ColumnMappingValue>> for ColumnMapping {
    fn from(map: HashMap<String, ColumnMappingValue>) -> Self {
        ColumnMapping::Explicit(map)
    }
}

impl std::ops::Deref for ColumnMapping {
    type Target = HashMap<String, ColumnMappingValue>;

    fn deref(&self) -> &Self::Target {
        self.as_map()
    }
}

impl Serialize for ColumnMapping {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            ColumnMapping::Auto => serializer.serialize_str("auto"),
            ColumnMapping::Inherited => serializer.serialize_str("inherited"),
            ColumnMapping::Explicit(map) => map.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ColumnMapping {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de;

        struct ColumnMappingVisitor;

        impl<'de> de::Visitor<'de> for ColumnMappingVisitor {
            type Value = ColumnMapping;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("\"auto\", \"inherited\", or a column mapping object")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<ColumnMapping, E> {
                match v {
                    "auto" => Ok(ColumnMapping::Auto),
                    "inherited" => Ok(ColumnMapping::Inherited),
                    _ => Err(E::custom(format!(
                        "expected \"auto\", \"inherited\", or a mapping object, got \"{}\"",
                        v
                    ))),
                }
            }

            fn visit_map<M: de::MapAccess<'de>>(self, map: M) -> Result<ColumnMapping, M::Error> {
                let inner =
                    HashMap::<String, ColumnMappingValue>::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(ColumnMapping::Explicit(inner))
            }
        }

        deserializer.deserialize_any(ColumnMappingVisitor)
    }
}

/// A literal value injected as a constant column.
///
/// String-only for now; extensible to integer/float/bool later.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LiteralValue {
    String(String),
}

/// Column mapping value: simple string, structured with grain, literal constant,
/// or anchored sub-name mapping for composed expressions.
#[derive(Debug, Clone)]
pub enum ColumnMappingValue {
    /// Direct column name mapping: `cost: adwords_cost`
    Simple(String),
    /// Column with temporal grain override: `{ column: created_at, grain: day }`
    WithGrain {
        column: String,
        grain: Option<TemporalGrain>,
    },
    /// Literal constant injection: `{ lit: "search" }`
    Literal(LiteralValue),
    /// Anchored sub-name mapping for composed expressions:
    /// `total_cost: { order_sum: physical_order_amount, delivery_cost: physical_delivery_fee }`
    Anchored(HashMap<String, String>),
}

impl Serialize for ColumnMappingValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeMap;
        match self {
            ColumnMappingValue::Simple(s) => serializer.serialize_str(s),
            ColumnMappingValue::WithGrain { column, grain } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("column", column)?;
                if let Some(g) = grain {
                    map.serialize_entry("grain", g)?;
                }
                map.end()
            }
            ColumnMappingValue::Literal(lit) => {
                let mut map = serializer.serialize_map(Some(1))?;
                match lit {
                    LiteralValue::String(s) => map.serialize_entry("lit", s)?,
                }
                map.end()
            }
            ColumnMappingValue::Anchored(anchors) => anchors.serialize(serializer),
        }
    }
}

/// Coerce a serde_yaml::Value to its string representation.
///
/// Handles the case where YAML scalars like `lit: 0` or `lit: true` are parsed
/// as non-string types. Converts them to canonical string form for storage
/// in `LiteralValue::String`.
fn yaml_value_to_string(v: &serde_yaml::Value) -> String {
    match v {
        serde_yaml::Value::String(s) => s.clone(),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.to_string()
            } else if let Some(f) = n.as_f64() {
                f.to_string()
            } else {
                n.to_string()
            }
        }
        serde_yaml::Value::Bool(b) => b.to_string(),
        serde_yaml::Value::Null => "null".to_string(),
        other => format!("{other:?}"),
    }
}

impl<'de> Deserialize<'de> for ColumnMappingValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WithGrainHelper {
            column: String,
            grain: Option<TemporalGrain>,
        }

        #[derive(Deserialize)]
        struct LiteralHelper {
            lit: serde_yaml::Value,
        }

        // Deserialize into a generic value first, then try each variant.
        let value = serde_yaml::Value::deserialize(deserializer)
            .map_err(serde::de::Error::custom)?;

        // String → Simple
        if let Some(s) = value.as_str() {
            return Ok(ColumnMappingValue::Simple(s.to_owned()));
        }

        // Object → try WithGrain (has "column" key), then Literal (has "lit" key),
        // then Anchored (catch-all object with string values).
        if value.is_mapping() {
            // Try WithGrain first (has required "column" key)
            if let Ok(wg) = serde_yaml::from_value::<WithGrainHelper>(value.clone()) {
                return Ok(ColumnMappingValue::WithGrain {
                    column: wg.column,
                    grain: wg.grain,
                });
            }
            // Try Literal (has required "lit" key)
            if let Ok(lit) = serde_yaml::from_value::<LiteralHelper>(value.clone()) {
                let string_val = yaml_value_to_string(&lit.lit);
                return Ok(ColumnMappingValue::Literal(LiteralValue::String(string_val)));
            }
            // Anchored: catch-all object with string → string entries
            if let Ok(map) = serde_yaml::from_value::<HashMap<String, String>>(value) {
                return Ok(ColumnMappingValue::Anchored(map));
            }
        }

        Err(serde::de::Error::custom(
            "expected a string, { column: ... }, { lit: ... }, or { anchor: column, ... }",
        ))
    }
}

// =============================================================================
// Ref Entry & Filter
// =============================================================================

/// Reference entry for reusable definitions (dimensions, measures, metrics).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RefEntry {
    #[serde(rename = "ref")]
    pub ref_name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MeasureFilter {
    pub name: String,
    pub expr: ExprSource,
}

/// Type alias — DatasetFilter and MeasureFilter are structurally identical.
/// Both have `name` and `expr` fields. Unified to MeasureFilter in SemanticInterface.
pub type DatasetFilter = MeasureFilter;

/// Convert a Vec of named items to a BTreeMap keyed by name.
pub fn vec_to_btreemap<T>(items: Vec<T>, key_fn: impl Fn(&T) -> String) -> BTreeMap<String, T> {
    items
        .into_iter()
        .map(|item| {
            let key = key_fn(&item);
            (key, item)
        })
        .collect()
}

/// Like `vec_to_btreemap` but returns an error on duplicate keys (SR-1/SR-3).
pub fn vec_to_btreemap_unique<T>(
    items: Vec<T>,
    key_fn: impl Fn(&T) -> String,
    container: &str,
    entity_kind: &str,
) -> Result<BTreeMap<String, T>, crate::ModelError> {
    let mut map = BTreeMap::new();
    for item in items {
        let key = key_fn(&item);
        if map.insert(key.clone(), item).is_some() {
            return Err(crate::ModelError::Validation(format!(
                "duplicate {} '{}' in '{}'",
                entity_kind, key, container
            )));
        }
    }
    Ok(map)
}

// =============================================================================
// Top-level SemanticModel
// =============================================================================

/// Root semantic model definition.
///
/// Kinds are represented as three implicit-type arrays in YAML:
/// `grainsets:`, `unionsets:`, `joinsets:`. After parsing, they are merged
/// into `entities: BTreeMap<String, DataKind>` for the rest of the pipeline.
#[derive(Debug, Clone, Serialize)]
pub struct SemanticModel {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_context: Option<AiContext>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    /// Catalog namespace for glob expansion (defaults to "default").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,

    /// All entities (datasets, grainsets, unionsets, joinsets) in a single map.
    /// Key is the entity name. Only top-level entities are queryable —
    /// inline datasets within complex entities are hidden implementation details.
    #[serde(skip)]
    pub entities: BTreeMap<String, super::data_kind::DataKind>,

    // Top-level relationships between datasets and/or kinds
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<Relationship>,

    // Reusable definitions (referenced via `ref:` syntax).
    // Stored as Vec for straightforward YAML deserialization;
    // looked up by name in resolve_refs() via HashMap.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dimensions: Vec<super::dimension::Dimension>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub measures: Vec<super::measure::Measure>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metrics: Vec<super::metric::Metric>,
}

// =============================================================================
// SemanticInterface — shared fields across all DataKind variants
// =============================================================================

/// Shared semantic interface fields across all DataKind variants.
///
/// Named collections use `BTreeMap<String, T>` for sorted, O(log n) lookup.
/// The key is the entry name (extracted via `name()` on each Entry enum).
#[derive(Debug, Clone, Serialize, Default)]
pub struct SemanticInterface {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_context: Option<AiContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keys: Option<Keys>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dimensions: BTreeMap<String, DimensionEntry>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub measures: BTreeMap<String, MeasureEntry>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metrics: BTreeMap<String, MetricEntry>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub filters: BTreeMap<String, MeasureFilter>,
}

/// Build a SemanticInterface from YAML Vec fields.
///
/// Returns an error if any dimension, measure, metric, or filter name
/// appears more than once within the same container (SR-1 / SR-3).
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_semantic_interface(
    container_name: &str,
    description: Option<String>,
    ai_context: Option<AiContext>,
    keys: Option<Keys>,
    dimensions: Vec<DimensionEntry>,
    measures: Vec<MeasureEntry>,
    metrics: Vec<MetricEntry>,
    filters: Vec<MeasureFilter>,
) -> Result<SemanticInterface, crate::ModelError> {
    Ok(SemanticInterface {
        description,
        ai_context,
        keys,
        dimensions: vec_to_btreemap_unique(dimensions, |d| d.name().to_string(), container_name, "dimension")?,
        measures: vec_to_btreemap_unique(measures, |m| m.name().to_string(), container_name, "measure")?,
        metrics: vec_to_btreemap_unique(metrics, |m| m.name().to_string(), container_name, "metric")?,
        filters: vec_to_btreemap_unique(filters, |f| f.name.clone(), container_name, "filter")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_string_value() {
        let yaml = r#"lit: "search""#;
        let v: ColumnMappingValue = serde_yaml::from_str(yaml).unwrap();
        match v {
            ColumnMappingValue::Literal(LiteralValue::String(s)) => assert_eq!(s, "search"),
            other => panic!("expected Literal(String), got {other:?}"),
        }
    }

    #[test]
    fn literal_unquoted_string_value() {
        let yaml = "lit: web";
        let v: ColumnMappingValue = serde_yaml::from_str(yaml).unwrap();
        match v {
            ColumnMappingValue::Literal(LiteralValue::String(s)) => assert_eq!(s, "web"),
            other => panic!("expected Literal(String('web')), got {other:?}"),
        }
    }

    #[test]
    fn literal_integer_value() {
        let yaml = "lit: 0";
        let v: ColumnMappingValue = serde_yaml::from_str(yaml).unwrap();
        match v {
            ColumnMappingValue::Literal(LiteralValue::String(s)) => assert_eq!(s, "0"),
            other => panic!("expected Literal(String('0')), got {other:?}"),
        }
    }

    #[test]
    fn literal_negative_integer_value() {
        let yaml = "lit: -42";
        let v: ColumnMappingValue = serde_yaml::from_str(yaml).unwrap();
        match v {
            ColumnMappingValue::Literal(LiteralValue::String(s)) => assert_eq!(s, "-42"),
            other => panic!("expected Literal(String('-42')), got {other:?}"),
        }
    }

    #[test]
    fn literal_float_value() {
        let yaml = "lit: 3.14";
        let v: ColumnMappingValue = serde_yaml::from_str(yaml).unwrap();
        match v {
            ColumnMappingValue::Literal(LiteralValue::String(s)) => assert_eq!(s, "3.14"),
            other => panic!("expected Literal(String('3.14')), got {other:?}"),
        }
    }

    #[test]
    fn literal_bool_value() {
        let yaml = "lit: true";
        let v: ColumnMappingValue = serde_yaml::from_str(yaml).unwrap();
        match v {
            ColumnMappingValue::Literal(LiteralValue::String(s)) => assert_eq!(s, "true"),
            other => panic!("expected Literal(String('true')), got {other:?}"),
        }
    }

    #[test]
    fn literal_null_value() {
        let yaml = "lit: null";
        let v: ColumnMappingValue = serde_yaml::from_str(yaml).unwrap();
        match v {
            ColumnMappingValue::Literal(LiteralValue::String(s)) => assert_eq!(s, "null"),
            other => panic!("expected Literal(String('null')), got {other:?}"),
        }
    }

    #[test]
    fn simple_mapping() {
        let yaml = "physical_col";
        let v: ColumnMappingValue = serde_yaml::from_str(yaml).unwrap();
        match v {
            ColumnMappingValue::Simple(s) => assert_eq!(s, "physical_col"),
            other => panic!("expected Simple, got {other:?}"),
        }
    }
}
