//! ANSI SQL logical data type system.
//!
//! Defines the semantic model's type vocabulary — 8 logical types aligned with
//! ANSI SQL standards. Physical type mapping (Int64 vs Int32, Utf8 vs LargeUtf8)
//! is the responsibility of engine adapters and connectors, not the semantic layer.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

/// ANSI SQL logical data types for the semantic model.
///
/// These represent business-level data categories, not physical storage formats.
/// Engine adapters map these to engine-specific types (e.g., `Integer` → Arrow Int64,
/// `String` → Arrow Utf8, `Number` → Arrow Float64).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DataType {
    /// Whole numbers (ANSI INTEGER/BIGINT). Engine maps to Int64.
    Integer,
    /// Floating-point numbers (ANSI DOUBLE PRECISION). Engine maps to Float64.
    Number,
    /// Fixed-precision decimal (ANSI DECIMAL(p,s)).
    Decimal { precision: u8, scale: i8 },
    /// Text values (ANSI VARCHAR). Engine maps to Utf8.
    String,
    /// True/false values (ANSI BOOLEAN).
    Boolean,
    /// Calendar dates without time (ANSI DATE). Engine maps to Date32.
    Date,
    /// Date+time with precision (ANSI TIMESTAMP(p)).
    /// precision: 0=seconds, 3=milliseconds, 6=microseconds.
    Timestamp { precision: u8 },
    /// Raw byte sequences (ANSI BLOB/BINARY).
    Binary,
}

impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataType::Integer => write!(f, "integer"),
            DataType::Number => write!(f, "number"),
            DataType::Decimal { precision, scale } => write!(f, "decimal({},{})", precision, scale),
            DataType::String => write!(f, "string"),
            DataType::Boolean => write!(f, "boolean"),
            DataType::Date => write!(f, "date"),
            DataType::Timestamp { precision } => write!(f, "timestamp({})", precision),
            DataType::Binary => write!(f, "binary"),
        }
    }
}

impl FromStr for DataType {
    type Err = std::string::String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lower = s.to_lowercase();

        // Handle decimal with precision and scale
        if lower.starts_with("decimal(") && lower.ends_with(')') {
            return parse_decimal(&lower);
        }

        // Handle timestamp with precision
        if lower.starts_with("timestamp(") && lower.ends_with(')') {
            return parse_timestamp(&lower);
        }

        match lower.as_str() {
            // Integer aliases
            "integer" | "int" | "bigint" | "long" | "i8" | "i16" | "i32" | "i64"
            | "int8" | "int16" | "int32" | "int64" | "tinyint" | "smallint" => {
                Ok(DataType::Integer)
            }
            // Number aliases (floating point)
            "number" | "float" | "double" | "f32" | "f64" | "float32" | "float64" => {
                Ok(DataType::Number)
            }
            // String aliases
            "string" | "text" | "varchar" | "utf8" | "large_utf8" | "largeutf8" => {
                Ok(DataType::String)
            }
            // Boolean aliases
            "bool" | "boolean" => Ok(DataType::Boolean),
            // Date aliases
            "date" | "date32" | "date64" => Ok(DataType::Date),
            // Timestamp aliases (default precision: microseconds)
            "timestamp" | "timestamp_s" | "timestamp_second" => {
                Ok(DataType::Timestamp { precision: 0 })
            }
            "timestamp_ms" | "timestamp_millisecond" => {
                Ok(DataType::Timestamp { precision: 3 })
            }
            "timestamp_us" | "timestamp_microsecond" => {
                Ok(DataType::Timestamp { precision: 6 })
            }
            // Binary
            "binary" => Ok(DataType::Binary),
            // Decimal without precision defaults
            "decimal" => Ok(DataType::Decimal {
                precision: 18,
                scale: 2,
            }),
            _ => Err(format!("Unknown data type: {}", s)),
        }
    }
}

fn parse_decimal(s: &str) -> Result<DataType, std::string::String> {
    let inner = &s[8..s.len() - 1];
    let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();

    if parts.len() != 2 {
        return Err(format!(
            "Decimal requires precision and scale, e.g., decimal(10,2): {}",
            s
        ));
    }

    let precision: u8 = parts[0]
        .parse()
        .map_err(|_| format!("Invalid precision in decimal: {}", s))?;

    let scale: i8 = parts[1]
        .parse()
        .map_err(|_| format!("Invalid scale in decimal: {}", s))?;

    if precision == 0 || precision > 38 {
        return Err(format!("Precision must be between 1 and 38: {}", s));
    }

    if scale < 0 {
        return Err(format!("Scale must be non-negative: {}", s));
    }

    if scale as u8 > precision {
        return Err(format!("Scale cannot exceed precision: {}", s));
    }

    Ok(DataType::Decimal { precision, scale })
}

fn parse_timestamp(s: &str) -> Result<DataType, std::string::String> {
    let inner = &s[10..s.len() - 1].trim();
    let precision: u8 = inner
        .parse()
        .map_err(|_| format!("Invalid timestamp precision: {}", s))?;

    if precision > 9 {
        return Err(format!(
            "Timestamp precision must be 0-9: {}",
            s
        ));
    }

    Ok(DataType::Timestamp { precision })
}

// Serialize to string representation
impl Serialize for DataType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

// Deserialize from string representation
impl<'de> Deserialize<'de> for DataType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = std::string::String::deserialize(deserializer)?;
        DataType::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl DataType {
    /// Check if this is a numeric type (Integer, Number, or Decimal).
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            DataType::Integer | DataType::Number | DataType::Decimal { .. }
        )
    }

    /// Check if this is an integer type.
    pub fn is_integer(&self) -> bool {
        matches!(self, DataType::Integer)
    }

    /// Check if this is a floating-point type (Number or Decimal).
    pub fn is_float(&self) -> bool {
        matches!(self, DataType::Number)
    }

    /// Check if this is a temporal type (Date or Timestamp).
    pub fn is_temporal(&self) -> bool {
        matches!(self, DataType::Date | DataType::Timestamp { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_logical_types() {
        assert_eq!("boolean".parse::<DataType>().unwrap(), DataType::Boolean);
        assert_eq!("integer".parse::<DataType>().unwrap(), DataType::Integer);
        assert_eq!("number".parse::<DataType>().unwrap(), DataType::Number);
        assert_eq!("string".parse::<DataType>().unwrap(), DataType::String);
        assert_eq!("date".parse::<DataType>().unwrap(), DataType::Date);
        assert_eq!("binary".parse::<DataType>().unwrap(), DataType::Binary);
    }

    #[test]
    fn test_parse_backward_compat_aliases() {
        // Old Arrow-style names still parse correctly
        assert_eq!("i32".parse::<DataType>().unwrap(), DataType::Integer);
        assert_eq!("i64".parse::<DataType>().unwrap(), DataType::Integer);
        assert_eq!("bigint".parse::<DataType>().unwrap(), DataType::Integer);
        assert_eq!("f64".parse::<DataType>().unwrap(), DataType::Number);
        assert_eq!("double".parse::<DataType>().unwrap(), DataType::Number);
        assert_eq!("utf8".parse::<DataType>().unwrap(), DataType::String);
        assert_eq!("date32".parse::<DataType>().unwrap(), DataType::Date);
        assert_eq!("bool".parse::<DataType>().unwrap(), DataType::Boolean);
    }

    #[test]
    fn test_parse_sql_aliases() {
        assert_eq!("int".parse::<DataType>().unwrap(), DataType::Integer);
        assert_eq!("float".parse::<DataType>().unwrap(), DataType::Number);
        assert_eq!("varchar".parse::<DataType>().unwrap(), DataType::String);
        assert_eq!("text".parse::<DataType>().unwrap(), DataType::String);
    }

    #[test]
    fn test_parse_decimal() {
        assert_eq!(
            "decimal(10,2)".parse::<DataType>().unwrap(),
            DataType::Decimal {
                precision: 10,
                scale: 2
            }
        );
        assert_eq!(
            "decimal(38,10)".parse::<DataType>().unwrap(),
            DataType::Decimal {
                precision: 38,
                scale: 10
            }
        );
    }

    #[test]
    fn test_parse_decimal_errors() {
        assert!("decimal(0,0)".parse::<DataType>().is_err());
        assert!("decimal(5,10)".parse::<DataType>().is_err());
        assert!("decimal(50,2)".parse::<DataType>().is_err());
    }

    #[test]
    fn test_parse_timestamp_precision() {
        assert_eq!(
            "timestamp(0)".parse::<DataType>().unwrap(),
            DataType::Timestamp { precision: 0 }
        );
        assert_eq!(
            "timestamp(3)".parse::<DataType>().unwrap(),
            DataType::Timestamp { precision: 3 }
        );
        assert_eq!(
            "timestamp(6)".parse::<DataType>().unwrap(),
            DataType::Timestamp { precision: 6 }
        );
    }

    #[test]
    fn test_parse_timestamp_aliases() {
        assert_eq!(
            "timestamp".parse::<DataType>().unwrap(),
            DataType::Timestamp { precision: 0 }
        );
        assert_eq!(
            "timestamp_ms".parse::<DataType>().unwrap(),
            DataType::Timestamp { precision: 3 }
        );
        assert_eq!(
            "timestamp_us".parse::<DataType>().unwrap(),
            DataType::Timestamp { precision: 6 }
        );
    }

    #[test]
    fn test_display() {
        assert_eq!(DataType::Integer.to_string(), "integer");
        assert_eq!(DataType::Number.to_string(), "number");
        assert_eq!(DataType::String.to_string(), "string");
        assert_eq!(DataType::Boolean.to_string(), "boolean");
        assert_eq!(DataType::Date.to_string(), "date");
        assert_eq!(DataType::Binary.to_string(), "binary");
        assert_eq!(
            DataType::Decimal {
                precision: 10,
                scale: 2
            }
            .to_string(),
            "decimal(10,2)"
        );
        assert_eq!(
            DataType::Timestamp { precision: 6 }.to_string(),
            "timestamp(6)"
        );
    }

    #[test]
    fn test_serde_roundtrip() {
        let types = vec![
            DataType::Boolean,
            DataType::Integer,
            DataType::Number,
            DataType::String,
            DataType::Date,
            DataType::Binary,
            DataType::Decimal {
                precision: 18,
                scale: 2,
            },
            DataType::Timestamp { precision: 6 },
        ];

        for dt in types {
            let json = serde_json::to_string(&dt).unwrap();
            let parsed: DataType = serde_json::from_str(&json).unwrap();
            assert_eq!(dt, parsed);
        }
    }

    #[test]
    fn test_type_predicates() {
        assert!(DataType::Integer.is_numeric());
        assert!(DataType::Integer.is_integer());
        assert!(!DataType::Integer.is_float());
        assert!(!DataType::Integer.is_temporal());

        assert!(DataType::Number.is_numeric());
        assert!(!DataType::Number.is_integer());
        assert!(DataType::Number.is_float());

        assert!(DataType::Decimal {
            precision: 10,
            scale: 2
        }
        .is_numeric());

        assert!(DataType::Date.is_temporal());
        assert!(DataType::Timestamp { precision: 3 }.is_temporal());
        assert!(!DataType::Date.is_numeric());

        assert!(!DataType::String.is_numeric());
        assert!(!DataType::String.is_temporal());
        assert!(!DataType::Boolean.is_numeric());
    }
}
