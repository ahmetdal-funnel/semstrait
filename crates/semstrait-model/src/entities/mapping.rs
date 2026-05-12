//! `SemanticMapping`, `SemanticMappingValue`, `LiteralValue` — `18 §10`.
//!
//! The `SemanticMapping` block lives inside `LeafExtras`. It maps each
//! Semantic name (Dimension / Measure / Metric / Filter that the leaf
//! exposes) to a physical-side carrier. Two authoring forms are
//! recognized:
//!
//! - **Auto** (`semantic_mapping: auto` or omitted) — every Semantic
//!   resolves 1:1 to a physical column with the same name.
//! - **Explicit map** — per-name dispatch into one of three author-
//!   facing variants (`Column`, `Literal`, `Expr`). The fourth variant
//!   on the type, `Metadata`, is **compile-synthesized only** from a
//!   Dimension's `type: { metadata: ... }` block (`18 §10`) and has no
//!   YAML representation under `semantic_mapping:`.

use crate::entities::physical_expr::PhysicalExpr;
use crate::types::SemanticsName;
use crate::yaml::tagged::single_key_map;
use indexmap::IndexMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_yaml::Value;

/// Top-level `semantic_mapping:` value. Holds either the implicit
/// `auto` default or an explicit per-Semantic map.
///
/// At the YAML surface this surfaces as either the string `"auto"` or
/// a mapping of `{ semantic_name: <SemanticMappingValue> }`. The
/// internal representation uses [`IndexMap`] so author order is
/// preserved (per `32 §7` ordering rules — within a leaf's mapping the
/// per-Semantic order is structurally significant for diagnostic
/// stability).
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub enum SemanticMapping {
    /// Implicit 1:1 column matching — semantic_name == column_name.
    #[default]
    Auto,
    /// Explicit per-Semantic mapping.
    Explicit(IndexMap<SemanticsName, SemanticMappingValue>),
}

impl SemanticMapping {
    /// True when this mapping resolves every name through the implicit
    /// 1:1 default.
    pub fn is_auto(&self) -> bool {
        matches!(self, Self::Auto)
    }

    pub fn explicit(entries: IndexMap<SemanticsName, SemanticMappingValue>) -> Self {
        Self::Explicit(entries)
    }

    /// Per-Semantic mapping builder per `32 §9.7.4`. Each insert call
    /// is 1:1 with a `SemanticMappingValue` variant — the
    /// compile-synthesized `Metadata` variant has no inserter (R3).
    pub fn builder() -> SemanticMappingBuilder {
        SemanticMappingBuilder::default()
    }
}

/// Hand-rolled per-variant inserter builder for [`SemanticMapping`].
/// Order of inserts is preserved via [`IndexMap`] (`32 §7`).
#[derive(Debug, Default)]
pub struct SemanticMappingBuilder {
    entries: IndexMap<SemanticsName, SemanticMappingValue>,
}

impl SemanticMappingBuilder {
    /// 1:1 with [`SemanticMappingValue::Column`].
    pub fn column(
        mut self,
        semantic: impl Into<SemanticsName>,
        column: impl Into<String>,
    ) -> Self {
        self.entries
            .insert(semantic.into(), SemanticMappingValue::Column(column.into()));
        self
    }

    /// 1:1 with [`SemanticMappingValue::Literal`].
    pub fn literal(mut self, semantic: impl Into<SemanticsName>, value: LiteralValue) -> Self {
        self.entries
            .insert(semantic.into(), SemanticMappingValue::Literal(value));
        self
    }

    /// 1:1 with [`SemanticMappingValue::Expr`].
    pub fn expr(mut self, semantic: impl Into<SemanticsName>, expr: PhysicalExpr) -> Self {
        self.entries
            .insert(semantic.into(), SemanticMappingValue::Expr(expr));
        self
    }

    pub fn build(self) -> SemanticMapping {
        if self.entries.is_empty() {
            SemanticMapping::Auto
        } else {
            SemanticMapping::Explicit(self.entries)
        }
    }
}

impl<'de> Deserialize<'de> for SemanticMapping {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        match value {
            Value::String(ref s) if s == "auto" => Ok(SemanticMapping::Auto),
            Value::Mapping(_) => {
                let entries: IndexMap<SemanticsName, SemanticMappingValue> =
                    serde_yaml::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(SemanticMapping::Explicit(entries))
            }
            other => Err(serde::de::Error::custom(format!(
                "semantic_mapping: expected `auto` or a mapping, got {:?}",
                other
            ))),
        }
    }
}

impl Serialize for SemanticMapping {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            SemanticMapping::Auto => serializer.serialize_str("auto"),
            SemanticMapping::Explicit(map) => map.serialize(serializer),
        }
    }
}

/// One mapping entry's right-hand value. Three variants are author-
/// facing; `Metadata` is compile-synthesized only.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum SemanticMappingValue {
    /// Bare physical column name — Semantic is 1:1 to `Column(name)`.
    Column(String),
    /// A literal broadcast over every row.
    Literal(LiteralValue),
    /// A `PhysicalExpr` tree — anything from a simple cast to a
    /// multi-column compute.
    Expr(PhysicalExpr),
    /// Compile-synthesized metadata-extraction recipe. Never authored
    /// under `semantic_mapping:` (per `18 §10.4`).
    Metadata(MetadataDimensionRecipe),
}

impl<'de> Deserialize<'de> for SemanticMappingValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        match value {
            Value::String(s) => Ok(SemanticMappingValue::Column(s)),
            Value::Mapping(ref map) => {
                if let Some(lit) = map.get(Value::String("literal".into())) {
                    let v: LiteralValue =
                        serde_yaml::from_value(lit.clone()).map_err(serde::de::Error::custom)?;
                    Ok(SemanticMappingValue::Literal(v))
                } else if let Some(expr_val) = map.get(Value::String("expr".into())) {
                    let pe: PhysicalExpr = serde_yaml::from_value(expr_val.clone())
                        .map_err(serde::de::Error::custom)?;
                    Ok(SemanticMappingValue::Expr(pe))
                } else {
                    Err(serde::de::Error::custom(
                        "semantic_mapping value: expected bare column string or `{literal: ...}` / `{expr: ...}` map"
                            .to_string(),
                    ))
                }
            }
            other => Err(serde::de::Error::custom(format!(
                "semantic_mapping value: unexpected YAML shape {:?}",
                other
            ))),
        }
    }
}

impl Serialize for SemanticMappingValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeMap;
        match self {
            Self::Column(s) => serializer.serialize_str(s),
            Self::Literal(l) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("literal", l)?;
                map.end()
            }
            Self::Expr(e) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("expr", e)?;
                map.end()
            }
            Self::Metadata(_) => Err(serde::ser::Error::custom(
                "SemanticMappingValue::Metadata is compile-synthesized; not serialised at the model author surface",
            )),
        }
    }
}

/// Literal payload for `SemanticMappingValue::Literal`. Each variant
/// carries the wire-typed kind tag the binder validates against the
/// Semantic's declared `data_type:` (per `18 §10.2`).
///
/// YAML form is the externally-tagged single-key map for the body
/// variants (`{int: 5}`, `{string: "USD"}`, …) and the bare string
/// `null` for the body-less variant. Hand-rolled `Deserialize` per
/// [`crate::yaml::tagged`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum LiteralValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    /// Decimal carried as a string for lossless round-trip.
    Decimal(String),
    String(String),
    /// ISO-8601 date.
    Date(String),
    /// ISO-8601 timestamp.
    Timestamp(String),
}

impl<'de> Deserialize<'de> for LiteralValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        match value {
            Value::String(ref s) if s == "null" => Ok(Self::Null),
            Value::Null => Ok(Self::Null),
            Value::Mapping(_) => {
                let (key, body) = single_key_map::<D::Error>(value, "LiteralValue")?;
                match key.as_str() {
                    "bool" => {
                        let v: bool =
                            serde_yaml::from_value(body).map_err(serde::de::Error::custom)?;
                        Ok(Self::Bool(v))
                    }
                    "int" => {
                        let v: i64 =
                            serde_yaml::from_value(body).map_err(serde::de::Error::custom)?;
                        Ok(Self::Int(v))
                    }
                    "float" => {
                        let v: f64 =
                            serde_yaml::from_value(body).map_err(serde::de::Error::custom)?;
                        Ok(Self::Float(v))
                    }
                    "decimal" => {
                        let v: String =
                            serde_yaml::from_value(body).map_err(serde::de::Error::custom)?;
                        Ok(Self::Decimal(v))
                    }
                    "string" => {
                        let v: String =
                            serde_yaml::from_value(body).map_err(serde::de::Error::custom)?;
                        Ok(Self::String(v))
                    }
                    "date" => {
                        let v: String =
                            serde_yaml::from_value(body).map_err(serde::de::Error::custom)?;
                        Ok(Self::Date(v))
                    }
                    "timestamp" => {
                        let v: String =
                            serde_yaml::from_value(body).map_err(serde::de::Error::custom)?;
                        Ok(Self::Timestamp(v))
                    }
                    "null" => Err(serde::de::Error::custom(
                        "LiteralValue::null is body-less; use the bare-string form `null`",
                    )),
                    other => Err(serde::de::Error::custom(format!(
                        "LiteralValue: unknown variant `{other}`"
                    ))),
                }
            }
            other => Err(serde::de::Error::custom(format!(
                "LiteralValue: expected bare `null` or single-key tagged map, got {other:?}"
            ))),
        }
    }
}

/// Compile-synthesized recipe for `SemanticMappingValue::Metadata`.
/// Never serialised at the author surface.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct MetadataDimensionRecipe {
    pub extraction: MetadataExtraction,
    pub data_type: semstrait_core::DataType,
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum MetadataExtraction {
    /// Extract token at 0-indexed (scheme-stripped) segment position.
    Path { token: u32 },
}
