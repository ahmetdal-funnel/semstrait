//! Canonical built-in catalog. Per `14a §4`.
//!
//! Each sub-module exposes a `pub(super) fn specs() -> Vec<FunctionSpec>`
//! returning that family's entries. [`assemble_core_specs`] folds them
//! into one flat list consumed by [`crate::functions::registry`] at
//! `OnceLock` bootstrap.

mod aggregate;
mod logical;
mod math;
mod string;
mod temporal;

use crate::expr_kinds::AggregationOp;
use crate::functions::spec::{Additivity, FunctionSpec};
use crate::types::DataType;

/// Build the v1 canonical catalog by family.
pub(super) fn assemble_core_specs() -> Vec<FunctionSpec> {
    let mut out = Vec::new();
    out.extend(string::specs());
    out.extend(math::specs());
    out.extend(temporal::specs());
    out.extend(logical::specs());
    out.extend(aggregate::specs());
    out
}

/// Closed-five additivity per `14a §4.7`. Reserved for Phase B Strategy
/// consumption; held here so adapters do not re-hardcode the table.
#[allow(dead_code)]
pub(crate) fn closed_five_additivity(op: AggregationOp) -> Additivity {
    match op {
        AggregationOp::Sum | AggregationOp::Count | AggregationOp::Min | AggregationOp::Max => {
            Additivity::Additive
        }
        AggregationOp::Avg => Additivity::NonAdditive,
    }
}

/// Closed-five SQL:2016 return-type promotion per `14a §4.7`.
///
/// - Sum/Avg: integer-family widens to Long; floating-family widens to
///   Double; Decimal(p,s) preserved.
/// - Count: always Long.
/// - Min/Max: same as input.
///
/// Non-numeric input to Sum/Avg falls back to the input type (the
/// signature layer rejects unsupported pairings; this helper is a
/// post-resolution lookup).
///
/// Reserved for Phase B Strategy consumption.
#[allow(dead_code)]
pub(crate) fn closed_five_return_type(op: AggregationOp, input: &DataType) -> DataType {
    match op {
        AggregationOp::Count => DataType::Long,
        AggregationOp::Min | AggregationOp::Max => input.clone(),
        AggregationOp::Sum | AggregationOp::Avg => promote_numeric(input),
    }
}

#[allow(dead_code)]
fn promote_numeric(input: &DataType) -> DataType {
    match input {
        DataType::Byte | DataType::Short | DataType::Integer | DataType::Long => DataType::Long,
        DataType::Float | DataType::Double => DataType::Double,
        DataType::Decimal { precision, scale } => DataType::Decimal {
            precision: *precision,
            scale: *scale,
        },
        other => other.clone(),
    }
}

/// One-line constructors used by every family file. Each helper builds
/// the most common shape so the catalog files read declaratively.
mod dsl {
    use crate::functions::canonical_fn::CanonicalFn;
    use crate::functions::spec::{
        Additivity, FnSignature, FunctionCategory, FunctionSpec, ParamType, ReturnTypeRule,
    };
    use crate::types::DataType;

    pub fn scalar(
        name: &str,
        signatures: Vec<FnSignature>,
        return_type: ReturnTypeRule,
    ) -> FunctionSpec {
        FunctionSpec {
            name: CanonicalFn::new(name).expect("valid catalog name"),
            category: FunctionCategory::Scalar,
            signatures,
            return_type,
            additivity: None,
        }
    }

    pub fn aggregate(
        name: &str,
        signatures: Vec<FnSignature>,
        return_type: ReturnTypeRule,
        additivity: Additivity,
    ) -> FunctionSpec {
        FunctionSpec {
            name: CanonicalFn::new(name).expect("valid catalog name"),
            category: FunctionCategory::Aggregate,
            signatures,
            return_type,
            additivity: Some(additivity),
        }
    }

    pub fn sig(params: Vec<ParamType>) -> FnSignature {
        FnSignature {
            params,
            variadic_tail: None,
        }
    }

    pub fn variadic(params: Vec<ParamType>, tail: ParamType) -> FnSignature {
        FnSignature {
            params,
            variadic_tail: Some(tail),
        }
    }

    pub fn p(t: DataType) -> ParamType {
        ParamType::Concrete(t)
    }

    pub fn p_any() -> ParamType {
        ParamType::Any
    }

    pub fn p_numeric() -> ParamType {
        ParamType::NumericFamily
    }

    pub fn p_string() -> ParamType {
        ParamType::StringFamily
    }

    pub fn p_decimal() -> ParamType {
        ParamType::DecimalFamily
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assemble_core_specs_is_non_empty() {
        let v = assemble_core_specs();
        assert!(!v.is_empty(), "catalog must contain at least one entry");
    }

    #[test]
    fn assemble_core_specs_has_no_empty_signatures() {
        for spec in assemble_core_specs() {
            assert!(
                !spec.signatures.is_empty(),
                "{} must declare at least one signature",
                spec.name.as_str()
            );
        }
    }

    #[test]
    fn assemble_core_specs_has_unique_names() {
        let v = assemble_core_specs();
        let mut seen = std::collections::HashSet::new();
        for spec in &v {
            let inserted = seen.insert(spec.name.as_str().to_string());
            assert!(
                inserted,
                "duplicate canonical name in catalog: {}",
                spec.name.as_str()
            );
        }
    }

    #[test]
    fn assemble_core_specs_aggregates_carry_additivity() {
        for spec in assemble_core_specs() {
            if spec.category == crate::functions::spec::FunctionCategory::Aggregate {
                assert!(
                    spec.additivity.is_some(),
                    "aggregate {} missing additivity",
                    spec.name.as_str()
                );
            }
        }
    }

    #[test]
    fn assemble_core_specs_scalars_have_no_additivity() {
        for spec in assemble_core_specs() {
            if spec.category == crate::functions::spec::FunctionCategory::Scalar {
                assert!(
                    spec.additivity.is_none(),
                    "scalar {} should not carry additivity",
                    spec.name.as_str()
                );
            }
        }
    }

    // ── P-9: closed-five lookup helpers ────────────────────────────────

    #[test]
    fn closed_five_additivity_avg_is_non_additive() {
        assert_eq!(
            closed_five_additivity(AggregationOp::Avg),
            Additivity::NonAdditive
        );
    }

    #[test]
    fn closed_five_additivity_others_are_additive() {
        for op in [
            AggregationOp::Sum,
            AggregationOp::Count,
            AggregationOp::Min,
            AggregationOp::Max,
        ] {
            assert_eq!(
                closed_five_additivity(op),
                Additivity::Additive,
                "{:?} must be Additive",
                op
            );
        }
    }

    #[test]
    fn closed_five_return_type_count_is_long() {
        for input in [
            DataType::Integer,
            DataType::String,
            DataType::Boolean,
            DataType::Date,
        ] {
            assert_eq!(
                closed_five_return_type(AggregationOp::Count, &input),
                DataType::Long,
                "Count over {:?} must return Long",
                input
            );
        }
    }

    #[test]
    fn closed_five_return_type_min_max_preserve_input() {
        for op in [AggregationOp::Min, AggregationOp::Max] {
            for input in [
                DataType::Byte,
                DataType::Integer,
                DataType::Long,
                DataType::Double,
                DataType::String,
                DataType::Date,
                DataType::Decimal {
                    precision: 18,
                    scale: 4,
                },
            ] {
                assert_eq!(
                    closed_five_return_type(op, &input),
                    input,
                    "{:?} over {:?} must preserve input",
                    op,
                    input
                );
            }
        }
    }

    #[test]
    fn closed_five_return_type_sum_avg_promote_integer_family_to_long() {
        for op in [AggregationOp::Sum, AggregationOp::Avg] {
            for input in [
                DataType::Byte,
                DataType::Short,
                DataType::Integer,
                DataType::Long,
            ] {
                assert_eq!(
                    closed_five_return_type(op, &input),
                    DataType::Long,
                    "{:?} over {:?} must promote to Long",
                    op,
                    input
                );
            }
        }
    }

    #[test]
    fn closed_five_return_type_sum_avg_promote_floating_family_to_double() {
        for op in [AggregationOp::Sum, AggregationOp::Avg] {
            for input in [DataType::Float, DataType::Double] {
                assert_eq!(
                    closed_five_return_type(op, &input),
                    DataType::Double,
                    "{:?} over {:?} must promote to Double",
                    op,
                    input
                );
            }
        }
    }

    #[test]
    fn closed_five_return_type_sum_avg_preserve_decimal() {
        for op in [AggregationOp::Sum, AggregationOp::Avg] {
            let input = DataType::Decimal {
                precision: 18,
                scale: 4,
            };
            assert_eq!(
                closed_five_return_type(op, &input),
                input,
                "{:?} over Decimal must preserve precision/scale",
                op
            );
        }
    }
}
