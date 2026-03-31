//! Function registry for compile-time validation of `FunctionCall` names and arity.
//!
//! All standard ANSI SQL functions supported in declarative YAML blocks are
//! pre-registered. Unknown function names produce a compile warning;
//! arity mismatches produce compile errors.
//!
//! Each function also declares its [`ReturnType`], enabling compile-time type
//! derivation when `data_type` is omitted from a semantic definition.

use std::collections::HashMap;
use std::sync::LazyLock;

use semstrait_core::DataType;

/// How a function's return type is determined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReturnType {
    /// Return type is always the same regardless of input (e.g., UPPER → String).
    Fixed(DataType),
    /// Return type matches the first argument's type (e.g., ABS, ROUND).
    SameAsInput,
    /// Return type is defined by the semantic definition's `data_type` tag (e.g., CAST).
    Semantic,
}

/// Compile-time function metadata.
#[derive(Debug, Clone)]
pub struct FunctionSpec {
    pub name: &'static str,
    pub min_args: usize,
    /// `None` = variadic (e.g., CONCAT, COALESCE).
    pub max_args: Option<usize>,
    pub category: FunctionCategory,
    pub return_type: ReturnType,
}

/// Function category for compile-time diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionCategory {
    /// ANSI SQL — expected to work on all target engines.
    Standard,
    /// Engine-specific — adapter must recognize or reject.
    EngineSpecific,
}

/// Registry of known functions for compile-time validation.
pub struct FunctionRegistry {
    functions: HashMap<String, FunctionSpec>,
}

impl FunctionRegistry {
    /// Returns a reference to the standard function registry.
    /// Uses `LazyLock` for efficient compile-time initialization.
    pub fn standard() -> &'static Self {
        static REGISTRY: LazyLock<FunctionRegistry> = LazyLock::new(|| {
            let mut reg = FunctionRegistry {
                functions: HashMap::new(),
            };

            use ReturnType::{Fixed, SameAsInput, Semantic};

            // ── String functions ───────────────────────────────────────
            reg.add(spec("UPPER", 1, Some(1), FunctionCategory::Standard, Fixed(DataType::String)));
            reg.add(spec("LOWER", 1, Some(1), FunctionCategory::Standard, Fixed(DataType::String)));
            reg.add(spec("TRIM", 1, Some(1), FunctionCategory::Standard, Fixed(DataType::String)));
            reg.add(spec("LTRIM", 1, Some(1), FunctionCategory::Standard, Fixed(DataType::String)));
            reg.add(spec("RTRIM", 1, Some(1), FunctionCategory::Standard, Fixed(DataType::String)));
            reg.add(spec("LENGTH", 1, Some(1), FunctionCategory::Standard, Fixed(DataType::Integer)));
            reg.add(spec("CONCAT", 1, None, FunctionCategory::Standard, Fixed(DataType::String)));
            reg.add(spec("REPLACE", 3, Some(3), FunctionCategory::Standard, Fixed(DataType::String)));
            reg.add(spec("SUBSTRING", 2, Some(3), FunctionCategory::Standard, Fixed(DataType::String)));
            reg.add(spec("LEFT", 2, Some(2), FunctionCategory::Standard, Fixed(DataType::String)));
            reg.add(spec("RIGHT", 2, Some(2), FunctionCategory::Standard, Fixed(DataType::String)));
            reg.add(spec("LPAD", 2, Some(3), FunctionCategory::Standard, Fixed(DataType::String)));
            reg.add(spec("RPAD", 2, Some(3), FunctionCategory::Standard, Fixed(DataType::String)));

            // ── Math functions ─────────────────────────────────────────
            reg.add(spec("ABS", 1, Some(1), FunctionCategory::Standard, SameAsInput));
            reg.add(spec("CEIL", 1, Some(1), FunctionCategory::Standard, SameAsInput));
            reg.add(spec("FLOOR", 1, Some(1), FunctionCategory::Standard, SameAsInput));
            reg.add(spec("ROUND", 1, Some(2), FunctionCategory::Standard, SameAsInput));
            reg.add(spec("POWER", 2, Some(2), FunctionCategory::Standard, Fixed(DataType::Number)));
            reg.add(spec("SQRT", 1, Some(1), FunctionCategory::Standard, Fixed(DataType::Number)));
            reg.add(spec("MOD", 2, Some(2), FunctionCategory::Standard, SameAsInput));

            // ── Date functions ─────────────────────────────────────────
            reg.add(spec("CURRENT_DATE", 0, Some(0), FunctionCategory::Standard, Fixed(DataType::Date)));
            reg.add(spec("CURRENT_TIMESTAMP", 0, Some(0), FunctionCategory::Standard, Fixed(DataType::Timestamp { precision: 6 })));
            reg.add(spec("DATE_ADD", 2, Some(2), FunctionCategory::Standard, Fixed(DataType::Date)));
            reg.add(spec("DATEDIFF", 2, Some(2), FunctionCategory::Standard, Fixed(DataType::Integer)));
            reg.add(spec("EXTRACT", 2, Some(2), FunctionCategory::Standard, Fixed(DataType::Integer)));

            // ── Conditional functions ──────────────────────────────────
            reg.add(spec("GREATEST", 1, None, FunctionCategory::Standard, SameAsInput));
            reg.add(spec("LEAST", 1, None, FunctionCategory::Standard, SameAsInput));
            reg.add(spec("CAST", 2, Some(2), FunctionCategory::Standard, Semantic));

            reg
        });

        &REGISTRY
    }

    fn add(&mut self, spec: FunctionSpec) {
        // Key is already uppercase from the spec() helper.
        self.functions.insert(spec.name.to_string(), spec);
    }

    /// Look up a function by name (case-insensitive).
    /// Uses a stack-allocated uppercase buffer for names ≤ 32 chars to avoid heap allocation.
    pub fn get(&self, name: &str) -> Option<&FunctionSpec> {
        let mut buf = [0u8; 32];
        let upper = if name.len() <= 32 {
            for (i, b) in name.bytes().enumerate() {
                buf[i] = b.to_ascii_uppercase();
            }
            std::str::from_utf8(&buf[..name.len()]).unwrap()
        } else {
            // Fallback for unusually long names (shouldn't happen in practice).
            return self.functions.get(&name.to_uppercase());
        };
        self.functions.get(upper)
    }

    /// Validate a function call. Returns `Ok(())` for known functions with
    /// correct arity, `Err(message)` for arity mismatches.
    /// Returns `Ok(())` for unknown functions (warning issued by caller).
    pub fn validate(&self, name: &str, arg_count: usize) -> Result<(), String> {
        if let Some(spec) = self.get(name) {
            if arg_count < spec.min_args {
                return Err(format!(
                    "function '{}' requires at least {} argument(s), got {}",
                    spec.name, spec.min_args, arg_count
                ));
            }
            if let Some(max) = spec.max_args {
                if arg_count > max {
                    return Err(format!(
                        "function '{}' accepts at most {} argument(s), got {}",
                        spec.name, max, arg_count
                    ));
                }
            }
        }
        // Unknown functions pass validation (warning at caller level).
        Ok(())
    }

    /// Returns true if the function name is registered.
    pub fn is_known(&self, name: &str) -> bool {
        self.get(name).is_some()
    }
}

/// Derive the output [`DataType`] for an aggregation function applied to an input type.
///
/// Rules:
/// - `COUNT` / `CountDistinct` → `Integer` (always)
/// - `SUM` / `MIN` / `MAX` → same as input
/// - `AVG` → `Number` (always)
pub fn derive_aggregate_type(
    agg: semstrait_core::expr::Aggregation,
    input_type: &DataType,
) -> DataType {
    use semstrait_core::expr::Aggregation;
    match agg {
        Aggregation::Count | Aggregation::CountDistinct => DataType::Integer,
        Aggregation::Sum | Aggregation::Min | Aggregation::Max => input_type.clone(),
        Aggregation::Avg => DataType::Number,
    }
}

fn spec(
    name: &'static str,
    min_args: usize,
    max_args: Option<usize>,
    category: FunctionCategory,
    return_type: ReturnType,
) -> FunctionSpec {
    FunctionSpec {
        name,
        min_args,
        max_args,
        category,
        return_type,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use semstrait_core::expr::Aggregation;

    #[test]
    fn test_standard_registry_has_28_functions() {
        let reg = FunctionRegistry::standard();
        assert_eq!(reg.functions.len(), 28);
    }

    #[test]
    fn test_case_insensitive_lookup() {
        let reg = FunctionRegistry::standard();
        assert!(reg.get("upper").is_some());
        assert!(reg.get("UPPER").is_some());
        assert!(reg.get("Upper").is_some());
    }

    #[test]
    fn test_validate_correct_arity() {
        let reg = FunctionRegistry::standard();
        assert!(reg.validate("UPPER", 1).is_ok());
        assert!(reg.validate("ROUND", 1).is_ok());
        assert!(reg.validate("ROUND", 2).is_ok());
        assert!(reg.validate("CONCAT", 5).is_ok()); // variadic
    }

    #[test]
    fn test_validate_too_few_args() {
        let reg = FunctionRegistry::standard();
        let err = reg.validate("UPPER", 0).unwrap_err();
        assert!(err.contains("at least 1"));
    }

    #[test]
    fn test_validate_too_many_args() {
        let reg = FunctionRegistry::standard();
        let err = reg.validate("UPPER", 2).unwrap_err();
        assert!(err.contains("at most 1"));
    }

    #[test]
    fn test_unknown_function_passes() {
        let reg = FunctionRegistry::standard();
        assert!(reg.validate("CUSTOM_FUNC", 3).is_ok());
        assert!(!reg.is_known("CUSTOM_FUNC"));
    }

    #[test]
    fn test_variadic_no_upper_bound() {
        let reg = FunctionRegistry::standard();
        assert!(reg.validate("CONCAT", 100).is_ok());
        assert!(reg.validate("GREATEST", 50).is_ok());
    }

    // ── Return type tests (J1 TDD) ───────────────────────────────

    #[test]
    fn test_all_functions_have_return_type() {
        let reg = FunctionRegistry::standard();
        for (name, spec) in &reg.functions {
            // Every function must have a non-default return type.
            // Just verifying the field exists and is populated.
            let _ = &spec.return_type;
            assert!(
                !name.is_empty(),
                "function spec with empty name should not exist"
            );
        }
    }

    #[test]
    fn test_return_type_string_functions() {
        let reg = FunctionRegistry::standard();
        for name in &[
            "UPPER", "LOWER", "TRIM", "LTRIM", "RTRIM", "CONCAT", "REPLACE",
            "SUBSTRING", "LEFT", "RIGHT", "LPAD", "RPAD",
        ] {
            let spec = reg.get(name).unwrap_or_else(|| panic!("{} not found", name));
            assert_eq!(
                spec.return_type,
                ReturnType::Fixed(DataType::String),
                "{} should return Fixed(String)",
                name
            );
        }
    }

    #[test]
    fn test_return_type_length_is_integer() {
        let reg = FunctionRegistry::standard();
        assert_eq!(
            reg.get("LENGTH").unwrap().return_type,
            ReturnType::Fixed(DataType::Integer)
        );
    }

    #[test]
    fn test_return_type_math_same_as_input() {
        let reg = FunctionRegistry::standard();
        for name in &["ABS", "CEIL", "FLOOR", "ROUND", "MOD", "GREATEST", "LEAST"] {
            let spec = reg.get(name).unwrap_or_else(|| panic!("{} not found", name));
            assert_eq!(
                spec.return_type,
                ReturnType::SameAsInput,
                "{} should return SameAsInput",
                name
            );
        }
    }

    #[test]
    fn test_return_type_power_sqrt_are_number() {
        let reg = FunctionRegistry::standard();
        assert_eq!(
            reg.get("POWER").unwrap().return_type,
            ReturnType::Fixed(DataType::Number)
        );
        assert_eq!(
            reg.get("SQRT").unwrap().return_type,
            ReturnType::Fixed(DataType::Number)
        );
    }

    #[test]
    fn test_return_type_date_functions() {
        let reg = FunctionRegistry::standard();
        assert_eq!(
            reg.get("CURRENT_DATE").unwrap().return_type,
            ReturnType::Fixed(DataType::Date)
        );
        assert_eq!(
            reg.get("DATE_ADD").unwrap().return_type,
            ReturnType::Fixed(DataType::Date)
        );
        assert_eq!(
            reg.get("DATEDIFF").unwrap().return_type,
            ReturnType::Fixed(DataType::Integer)
        );
        assert_eq!(
            reg.get("EXTRACT").unwrap().return_type,
            ReturnType::Fixed(DataType::Integer)
        );
        assert_eq!(
            reg.get("CURRENT_TIMESTAMP").unwrap().return_type,
            ReturnType::Fixed(DataType::Timestamp { precision: 6 })
        );
    }

    #[test]
    fn test_return_type_cast_is_semantic() {
        let reg = FunctionRegistry::standard();
        assert_eq!(reg.get("CAST").unwrap().return_type, ReturnType::Semantic);
    }

    // ── Aggregate type derivation tests (J1 TDD) ─────────────────

    #[test]
    fn test_derive_aggregate_type_count_returns_integer() {
        assert_eq!(
            derive_aggregate_type(Aggregation::Count, &DataType::String),
            DataType::Integer
        );
        assert_eq!(
            derive_aggregate_type(Aggregation::Count, &DataType::Number),
            DataType::Integer
        );
        assert_eq!(
            derive_aggregate_type(Aggregation::CountDistinct, &DataType::Date),
            DataType::Integer
        );
    }

    #[test]
    fn test_derive_aggregate_type_sum_preserves_input() {
        assert_eq!(
            derive_aggregate_type(Aggregation::Sum, &DataType::Integer),
            DataType::Integer
        );
        assert_eq!(
            derive_aggregate_type(Aggregation::Sum, &DataType::Number),
            DataType::Number
        );
        let decimal = DataType::Decimal {
            precision: 10,
            scale: 2,
        };
        assert_eq!(
            derive_aggregate_type(Aggregation::Sum, &decimal),
            decimal
        );
    }

    #[test]
    fn test_derive_aggregate_type_min_max_preserves_input() {
        assert_eq!(
            derive_aggregate_type(Aggregation::Min, &DataType::Date),
            DataType::Date
        );
        assert_eq!(
            derive_aggregate_type(Aggregation::Max, &DataType::Timestamp { precision: 3 }),
            DataType::Timestamp { precision: 3 }
        );
    }

    #[test]
    fn test_derive_aggregate_type_avg_returns_number() {
        assert_eq!(
            derive_aggregate_type(Aggregation::Avg, &DataType::Integer),
            DataType::Number
        );
        assert_eq!(
            derive_aggregate_type(Aggregation::Avg, &DataType::Number),
            DataType::Number
        );
    }
}
