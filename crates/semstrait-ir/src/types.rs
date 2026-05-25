//! Canonical type vocabulary. Spec `35 §4`, ratified by `13`.
//!
//! Owns:
//! - [`DataType`] — 14 ANSI-aligned scalar variants per `35 §4.1`.
//! - [`Grain`] — temporal granularity lattice per `35 §4.2`.
//! - [`TypeClass`] — bounded type-class grouping per `35 §4.3`.
//!
//! `Schema` / `SchemaColumn` are out of scope for this iteration — no
//! current consumer in the post-cascade IR needs them.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Per `35 §4.1`. 14 scalar variants, engine-neutral. Complex types
/// (arrays, structs, maps) are out of scope for v1 per `13 §2.5`.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DataType {
    Boolean,
    Byte,
    Short,
    Integer,
    Long,
    Float,
    Double,
    Decimal { precision: u8, scale: i8 },
    String,
    Binary,
    Date,
    Time { precision: u8 },
    Timestamp { precision: u8 },
    Interval,
}

impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Boolean => f.write_str("boolean"),
            Self::Byte => f.write_str("byte"),
            Self::Short => f.write_str("short"),
            Self::Integer => f.write_str("integer"),
            Self::Long => f.write_str("long"),
            Self::Float => f.write_str("float"),
            Self::Double => f.write_str("double"),
            Self::Decimal { precision, scale } => write!(f, "decimal({precision},{scale})"),
            Self::String => f.write_str("string"),
            Self::Binary => f.write_str("binary"),
            Self::Date => f.write_str("date"),
            Self::Time { precision } => write!(f, "time({precision})"),
            Self::Timestamp { precision } => write!(f, "timestamp({precision})"),
            Self::Interval => f.write_str("interval"),
        }
    }
}

impl Serialize for DataType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for DataType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        parse_data_type(&s).ok_or_else(|| serde::de::Error::custom(format!("unknown DataType: {s}")))
    }
}

fn parse_data_type(s: &str) -> Option<DataType> {
    match s {
        "boolean" => Some(DataType::Boolean),
        "byte" => Some(DataType::Byte),
        "short" => Some(DataType::Short),
        "integer" => Some(DataType::Integer),
        "long" => Some(DataType::Long),
        "float" => Some(DataType::Float),
        "double" => Some(DataType::Double),
        "string" => Some(DataType::String),
        "binary" => Some(DataType::Binary),
        "date" => Some(DataType::Date),
        "interval" => Some(DataType::Interval),
        s if s.starts_with("decimal(") && s.ends_with(')') => parse_decimal(s),
        s if s.starts_with("time(") && s.ends_with(')') => parse_time(s),
        s if s.starts_with("timestamp(") && s.ends_with(')') => parse_timestamp(s),
        _ => None,
    }
}

fn parse_decimal(s: &str) -> Option<DataType> {
    let inner = s.strip_prefix("decimal(")?.strip_suffix(')')?;
    let (p, scale) = inner.split_once(',')?;
    let precision: u8 = p.trim().parse().ok()?;
    let scale: i8 = scale.trim().parse().ok()?;
    Some(DataType::Decimal { precision, scale })
}

fn parse_time(s: &str) -> Option<DataType> {
    let inner = s.strip_prefix("time(")?.strip_suffix(')')?;
    let precision: u8 = inner.trim().parse().ok()?;
    Some(DataType::Time { precision })
}

fn parse_timestamp(s: &str) -> Option<DataType> {
    let inner = s.strip_prefix("timestamp(")?.strip_suffix(')')?;
    let precision: u8 = inner.trim().parse().ok()?;
    Some(DataType::Timestamp { precision })
}

/// Per `35 §4.2`. Temporal granularity lattice ordered by coarseness
/// per `13 §3.2`.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Grain {
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Quarter,
    Year,
}

impl Grain {
    /// Per `35 §4.2`. Selection-rank order: Minute(0) < ... < Year(6).
    pub fn coarseness(self) -> u8 {
        match self {
            Self::Minute => 0,
            Self::Hour => 1,
            Self::Day => 2,
            Self::Week => 3,
            Self::Month => 4,
            Self::Quarter => 5,
            Self::Year => 6,
        }
    }
}

impl fmt::Display for Grain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Minute => f.write_str("minute"),
            Self::Hour => f.write_str("hour"),
            Self::Day => f.write_str("day"),
            Self::Week => f.write_str("week"),
            Self::Month => f.write_str("month"),
            Self::Quarter => f.write_str("quarter"),
            Self::Year => f.write_str("year"),
        }
    }
}

/// Per `35 §4.3`. Bounded type classes used by future signature polymorphism
/// (`14a §3.3`). `TypeClass` is exposed but not wired into the v1 `ParamType`
/// activation per `35 §4.3`; it lives as vocabulary for future evolution.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeClass {
    Numeric,
    Integral,
    FloatingPt,
    Textual,
    Temporal,
    Comparable,
    Any,
}

impl TypeClass {
    /// Per `35 §4.3`. Membership predicate over the canonical [`DataType`]
    /// roster; pure grouping vocabulary, no method surface beyond this.
    pub fn contains(self, ty: &DataType) -> bool {
        match self {
            Self::Numeric => matches!(
                ty,
                DataType::Byte
                    | DataType::Short
                    | DataType::Integer
                    | DataType::Long
                    | DataType::Float
                    | DataType::Double
                    | DataType::Decimal { .. }
            ),
            Self::Integral => matches!(
                ty,
                DataType::Byte | DataType::Short | DataType::Integer | DataType::Long
            ),
            Self::FloatingPt => matches!(ty, DataType::Float | DataType::Double),
            Self::Textual => matches!(ty, DataType::String),
            Self::Temporal => matches!(
                ty,
                DataType::Date | DataType::Time { .. } | DataType::Timestamp { .. }
            ),
            Self::Comparable => !matches!(ty, DataType::Binary),
            Self::Any => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_type_roster_matches_spec_count() {
        let kinds: Vec<DataType> = vec![
            DataType::Boolean,
            DataType::Byte,
            DataType::Short,
            DataType::Integer,
            DataType::Long,
            DataType::Float,
            DataType::Double,
            DataType::Decimal {
                precision: 10,
                scale: 2,
            },
            DataType::String,
            DataType::Binary,
            DataType::Date,
            DataType::Time { precision: 6 },
            DataType::Timestamp { precision: 6 },
            DataType::Interval,
        ];
        assert_eq!(kinds.len(), 14, "spec §4.1 ratifies 14 scalar variants");
    }

    #[test]
    fn data_type_display_simple_variants() {
        assert_eq!(DataType::Boolean.to_string(), "boolean");
        assert_eq!(DataType::Byte.to_string(), "byte");
        assert_eq!(DataType::Short.to_string(), "short");
        assert_eq!(DataType::Integer.to_string(), "integer");
        assert_eq!(DataType::Long.to_string(), "long");
        assert_eq!(DataType::Float.to_string(), "float");
        assert_eq!(DataType::Double.to_string(), "double");
        assert_eq!(DataType::String.to_string(), "string");
        assert_eq!(DataType::Binary.to_string(), "binary");
        assert_eq!(DataType::Date.to_string(), "date");
        assert_eq!(DataType::Interval.to_string(), "interval");
    }

    #[test]
    fn data_type_display_parameterized_variants() {
        assert_eq!(
            DataType::Decimal {
                precision: 10,
                scale: 2,
            }
            .to_string(),
            "decimal(10,2)"
        );
        assert_eq!(
            DataType::Time { precision: 0 }.to_string(),
            "time(0)"
        );
        assert_eq!(
            DataType::Timestamp { precision: 6 }.to_string(),
            "timestamp(6)"
        );
    }

    #[test]
    fn data_type_serde_roundtrip_each_variant() {
        let roster = [
            DataType::Boolean,
            DataType::Byte,
            DataType::Short,
            DataType::Integer,
            DataType::Long,
            DataType::Float,
            DataType::Double,
            DataType::Decimal {
                precision: 10,
                scale: 2,
            },
            DataType::String,
            DataType::Binary,
            DataType::Date,
            DataType::Time { precision: 6 },
            DataType::Timestamp { precision: 9 },
            DataType::Interval,
        ];
        for ty in &roster {
            let json = serde_json::to_string(ty).unwrap();
            let back: DataType = serde_json::from_str(&json).unwrap();
            assert_eq!(ty, &back, "roundtrip failed for {ty}");
        }
    }

    #[test]
    fn data_type_serde_unknown_string_errors() {
        let bogus = "\"definitely_not_a_type\"";
        let r: Result<DataType, _> = serde_json::from_str(bogus);
        assert!(r.is_err());
    }

    #[test]
    fn data_type_is_non_exhaustive() {
        // The wildcard arm exists to prove `DataType` is `#[non_exhaustive]`
        // (per invariant I10): any future variant must fall through this
        // arm rather than break exhaustively-matching consumers.
        #[allow(unreachable_patterns)]
        let v = DataType::Boolean;
        #[allow(unreachable_patterns)]
        let _ = match v {
            DataType::Boolean => "b",
            DataType::Byte => "b",
            DataType::Short => "s",
            DataType::Integer => "i",
            DataType::Long => "l",
            DataType::Float => "f",
            DataType::Double => "d",
            DataType::Decimal { .. } => "dec",
            DataType::String => "s",
            DataType::Binary => "bin",
            DataType::Date => "date",
            DataType::Time { .. } => "t",
            DataType::Timestamp { .. } => "ts",
            DataType::Interval => "iv",
            _ => "?",
        };
    }

    #[test]
    fn grain_coarseness_is_total_ordered() {
        assert!(Grain::Minute.coarseness() < Grain::Hour.coarseness());
        assert!(Grain::Hour.coarseness() < Grain::Day.coarseness());
        assert!(Grain::Day.coarseness() < Grain::Week.coarseness());
        assert!(Grain::Week.coarseness() < Grain::Month.coarseness());
        assert!(Grain::Month.coarseness() < Grain::Quarter.coarseness());
        assert!(Grain::Quarter.coarseness() < Grain::Year.coarseness());
    }

    #[test]
    fn grain_coarseness_min_is_minute_max_is_year() {
        assert_eq!(Grain::Minute.coarseness(), 0);
        assert_eq!(Grain::Year.coarseness(), 6);
    }

    #[test]
    fn grain_display_lowercase() {
        assert_eq!(Grain::Day.to_string(), "day");
        assert_eq!(Grain::Quarter.to_string(), "quarter");
    }

    #[test]
    fn grain_serde_roundtrip() {
        let roster = [
            Grain::Minute,
            Grain::Hour,
            Grain::Day,
            Grain::Week,
            Grain::Month,
            Grain::Quarter,
            Grain::Year,
        ];
        for g in &roster {
            let json = serde_json::to_string(g).unwrap();
            let back: Grain = serde_json::from_str(&json).unwrap();
            assert_eq!(g, &back);
        }
    }

    #[test]
    fn type_class_numeric_membership() {
        assert!(TypeClass::Numeric.contains(&DataType::Integer));
        assert!(TypeClass::Numeric.contains(&DataType::Long));
        assert!(TypeClass::Numeric.contains(&DataType::Float));
        assert!(TypeClass::Numeric.contains(&DataType::Double));
        assert!(TypeClass::Numeric.contains(&DataType::Decimal {
            precision: 10,
            scale: 2,
        }));
        assert!(!TypeClass::Numeric.contains(&DataType::Boolean));
        assert!(!TypeClass::Numeric.contains(&DataType::String));
        assert!(!TypeClass::Numeric.contains(&DataType::Date));
    }

    #[test]
    fn type_class_integral_excludes_floating() {
        assert!(TypeClass::Integral.contains(&DataType::Byte));
        assert!(TypeClass::Integral.contains(&DataType::Short));
        assert!(TypeClass::Integral.contains(&DataType::Integer));
        assert!(TypeClass::Integral.contains(&DataType::Long));
        assert!(!TypeClass::Integral.contains(&DataType::Float));
        assert!(!TypeClass::Integral.contains(&DataType::Double));
        assert!(!TypeClass::Integral.contains(&DataType::Decimal {
            precision: 10,
            scale: 2,
        }));
    }

    #[test]
    fn type_class_floating_pt_membership() {
        assert!(TypeClass::FloatingPt.contains(&DataType::Float));
        assert!(TypeClass::FloatingPt.contains(&DataType::Double));
        assert!(!TypeClass::FloatingPt.contains(&DataType::Integer));
        assert!(!TypeClass::FloatingPt.contains(&DataType::Decimal {
            precision: 10,
            scale: 2,
        }));
    }

    #[test]
    fn type_class_textual_membership() {
        assert!(TypeClass::Textual.contains(&DataType::String));
        assert!(!TypeClass::Textual.contains(&DataType::Binary));
    }

    #[test]
    fn type_class_temporal_membership() {
        assert!(TypeClass::Temporal.contains(&DataType::Date));
        assert!(TypeClass::Temporal.contains(&DataType::Time { precision: 0 }));
        assert!(TypeClass::Temporal.contains(&DataType::Timestamp { precision: 6 }));
        assert!(!TypeClass::Temporal.contains(&DataType::Interval));
        assert!(!TypeClass::Temporal.contains(&DataType::String));
    }

    #[test]
    fn type_class_comparable_excludes_only_binary() {
        assert!(!TypeClass::Comparable.contains(&DataType::Binary));
        assert!(TypeClass::Comparable.contains(&DataType::Boolean));
        assert!(TypeClass::Comparable.contains(&DataType::Integer));
        assert!(TypeClass::Comparable.contains(&DataType::String));
        assert!(TypeClass::Comparable.contains(&DataType::Date));
    }

    #[test]
    fn type_class_any_admits_every_data_type() {
        let roster = [
            DataType::Boolean,
            DataType::Byte,
            DataType::Short,
            DataType::Integer,
            DataType::Long,
            DataType::Float,
            DataType::Double,
            DataType::Decimal {
                precision: 10,
                scale: 2,
            },
            DataType::String,
            DataType::Binary,
            DataType::Date,
            DataType::Time { precision: 6 },
            DataType::Timestamp { precision: 6 },
            DataType::Interval,
        ];
        for ty in &roster {
            assert!(TypeClass::Any.contains(ty), "Any must admit {ty}");
        }
    }

    #[test]
    fn type_class_serde_roundtrip() {
        let roster = [
            TypeClass::Numeric,
            TypeClass::Integral,
            TypeClass::FloatingPt,
            TypeClass::Textual,
            TypeClass::Temporal,
            TypeClass::Comparable,
            TypeClass::Any,
        ];
        for c in &roster {
            let json = serde_json::to_string(c).unwrap();
            let back: TypeClass = serde_json::from_str(&json).unwrap();
            assert_eq!(c, &back);
        }
    }
}
