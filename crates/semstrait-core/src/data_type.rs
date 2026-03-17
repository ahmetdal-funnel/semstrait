//! Arrow-aligned data type system.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

/// Arrow-aligned data type system.
/// Bidirectional compatibility with arrow::datatypes::DataType.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DataType {
    Boolean,
    Int8,
    Int16,
    Int32,
    Int64,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    Float32,
    Float64,
    Decimal { precision: u8, scale: i8 },
    Utf8,
    LargeUtf8,
    Date32,
    Date64,
    TimestampSecond,
    TimestampMillisecond,
    TimestampMicrosecond,
    Duration,
    List(Box<DataType>),
    Struct(Vec<StructField>),
    Binary,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StructField {
    pub name: String,
    pub data_type: DataType,
    pub nullable: bool,
}

impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataType::Boolean => write!(f, "bool"),
            DataType::Int8 => write!(f, "i8"),
            DataType::Int16 => write!(f, "i16"),
            DataType::Int32 => write!(f, "i32"),
            DataType::Int64 => write!(f, "i64"),
            DataType::UInt8 => write!(f, "u8"),
            DataType::UInt16 => write!(f, "u16"),
            DataType::UInt32 => write!(f, "u32"),
            DataType::UInt64 => write!(f, "u64"),
            DataType::Float32 => write!(f, "f32"),
            DataType::Float64 => write!(f, "f64"),
            DataType::Decimal { precision, scale } => write!(f, "decimal({},{})", precision, scale),
            DataType::Utf8 => write!(f, "utf8"),
            DataType::LargeUtf8 => write!(f, "large_utf8"),
            DataType::Date32 => write!(f, "date32"),
            DataType::Date64 => write!(f, "date64"),
            DataType::TimestampSecond => write!(f, "timestamp_s"),
            DataType::TimestampMillisecond => write!(f, "timestamp_ms"),
            DataType::TimestampMicrosecond => write!(f, "timestamp_us"),
            DataType::Duration => write!(f, "duration"),
            DataType::List(inner) => write!(f, "list({})", inner),
            DataType::Struct(fields) => {
                write!(f, "struct(")?;
                for (i, field) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, "{}:{}", field.name, field.data_type)?;
                }
                write!(f, ")")
            }
            DataType::Binary => write!(f, "binary"),
        }
    }
}

impl FromStr for DataType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lower = s.to_lowercase();

        // Handle decimal with precision and scale
        if lower.starts_with("decimal(") && lower.ends_with(')') {
            return parse_decimal(&lower);
        }

        // Handle list types
        if lower.starts_with("list(") && lower.ends_with(')') {
            let inner = &s[5..s.len() - 1];
            let inner_type = DataType::from_str(inner)?;
            return Ok(DataType::List(Box::new(inner_type)));
        }

        match lower.as_str() {
            "bool" | "boolean" => Ok(DataType::Boolean),
            "i8" | "int8" | "tinyint" => Ok(DataType::Int8),
            "i16" | "int16" | "smallint" => Ok(DataType::Int16),
            "i32" | "int32" | "int" | "integer" => Ok(DataType::Int32),
            "i64" | "int64" | "bigint" | "long" => Ok(DataType::Int64),
            "u8" | "uint8" => Ok(DataType::UInt8),
            "u16" | "uint16" => Ok(DataType::UInt16),
            "u32" | "uint32" => Ok(DataType::UInt32),
            "u64" | "uint64" => Ok(DataType::UInt64),
            "f32" | "float32" | "float" => Ok(DataType::Float32),
            "f64" | "float64" | "double" => Ok(DataType::Float64),
            "utf8" | "string" | "text" | "varchar" => Ok(DataType::Utf8),
            "large_utf8" | "largeutf8" => Ok(DataType::LargeUtf8),
            "date32" | "date" => Ok(DataType::Date32),
            "date64" => Ok(DataType::Date64),
            "timestamp_s" | "timestamp_second" => Ok(DataType::TimestampSecond),
            "timestamp_ms" | "timestamp_millisecond" | "timestamp" => Ok(DataType::TimestampMillisecond),
            "timestamp_us" | "timestamp_microsecond" => Ok(DataType::TimestampMicrosecond),
            "duration" => Ok(DataType::Duration),
            "binary" => Ok(DataType::Binary),
            _ => Err(format!("Unknown data type: {}", s)),
        }
    }
}

fn parse_decimal(s: &str) -> Result<DataType, String> {
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
        let s = String::deserialize(deserializer)?;
        DataType::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl DataType {
    /// Check if this is a numeric type.
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            DataType::Int8
                | DataType::Int16
                | DataType::Int32
                | DataType::Int64
                | DataType::UInt8
                | DataType::UInt16
                | DataType::UInt32
                | DataType::UInt64
                | DataType::Float32
                | DataType::Float64
                | DataType::Decimal { .. }
        )
    }

    /// Check if this is an integer type.
    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            DataType::Int8
                | DataType::Int16
                | DataType::Int32
                | DataType::Int64
                | DataType::UInt8
                | DataType::UInt16
                | DataType::UInt32
                | DataType::UInt64
        )
    }

    /// Check if this is a floating point type.
    pub fn is_float(&self) -> bool {
        matches!(self, DataType::Float32 | DataType::Float64)
    }

    /// Check if this is a temporal type.
    pub fn is_temporal(&self) -> bool {
        matches!(
            self,
            DataType::Date32
                | DataType::Date64
                | DataType::TimestampSecond
                | DataType::TimestampMillisecond
                | DataType::TimestampMicrosecond
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_types() {
        assert_eq!("bool".parse::<DataType>().unwrap(), DataType::Boolean);
        assert_eq!("i32".parse::<DataType>().unwrap(), DataType::Int32);
        assert_eq!("i64".parse::<DataType>().unwrap(), DataType::Int64);
        assert_eq!("f32".parse::<DataType>().unwrap(), DataType::Float32);
        assert_eq!("f64".parse::<DataType>().unwrap(), DataType::Float64);
        assert_eq!("utf8".parse::<DataType>().unwrap(), DataType::Utf8);
        assert_eq!("date32".parse::<DataType>().unwrap(), DataType::Date32);
    }

    #[test]
    fn test_parse_aliases() {
        assert_eq!("int".parse::<DataType>().unwrap(), DataType::Int32);
        assert_eq!("bigint".parse::<DataType>().unwrap(), DataType::Int64);
        assert_eq!("float".parse::<DataType>().unwrap(), DataType::Float32);
        assert_eq!("double".parse::<DataType>().unwrap(), DataType::Float64);
        assert_eq!("string".parse::<DataType>().unwrap(), DataType::Utf8);
        assert_eq!("text".parse::<DataType>().unwrap(), DataType::Utf8);
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
        assert!("decimal(0,0)".parse::<DataType>().is_err()); // precision = 0
        assert!("decimal(5,10)".parse::<DataType>().is_err()); // scale > precision
        assert!("decimal(50,2)".parse::<DataType>().is_err()); // precision > 38
    }

    #[test]
    fn test_parse_list() {
        assert_eq!(
            "list(i32)".parse::<DataType>().unwrap(),
            DataType::List(Box::new(DataType::Int32))
        );
        assert_eq!(
            "list(utf8)".parse::<DataType>().unwrap(),
            DataType::List(Box::new(DataType::Utf8))
        );
    }

    #[test]
    fn test_display() {
        assert_eq!(DataType::Int32.to_string(), "i32");
        assert_eq!(DataType::Utf8.to_string(), "utf8");
        assert_eq!(
            DataType::Decimal {
                precision: 10,
                scale: 2
            }
            .to_string(),
            "decimal(10,2)"
        );
        assert_eq!(
            DataType::List(Box::new(DataType::Int32)).to_string(),
            "list(i32)"
        );
    }

    #[test]
    fn test_serde_roundtrip() {
        let types = vec![
            DataType::Boolean,
            DataType::Int32,
            DataType::Float64,
            DataType::Utf8,
            DataType::Decimal {
                precision: 18,
                scale: 2,
            },
            DataType::List(Box::new(DataType::Int32)),
        ];

        for dt in types {
            let json = serde_json::to_string(&dt).unwrap();
            let parsed: DataType = serde_json::from_str(&json).unwrap();
            assert_eq!(dt, parsed);
        }
    }

    #[test]
    fn test_type_predicates() {
        assert!(DataType::Int32.is_numeric());
        assert!(DataType::Int32.is_integer());
        assert!(!DataType::Int32.is_float());
        assert!(!DataType::Int32.is_temporal());

        assert!(DataType::Float64.is_numeric());
        assert!(!DataType::Float64.is_integer());
        assert!(DataType::Float64.is_float());

        assert!(DataType::Decimal {
            precision: 10,
            scale: 2
        }
        .is_numeric());

        assert!(DataType::Date32.is_temporal());
        assert!(DataType::TimestampMillisecond.is_temporal());
        assert!(!DataType::Date32.is_numeric());

        assert!(!DataType::Utf8.is_numeric());
        assert!(!DataType::Utf8.is_temporal());
        assert!(!DataType::Boolean.is_numeric());
    }
}
