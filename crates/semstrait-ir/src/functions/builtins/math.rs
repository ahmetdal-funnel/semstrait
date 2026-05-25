//! Scalar — Math. Per `14a §4.3`.
//!
//! 11 entries. `mod` is NOT in the catalog — `BinaryOpKind::Mod` is the
//! canonical form.

use crate::functions::builtins::dsl::{p, p_numeric, scalar, sig};
use crate::functions::spec::{FunctionSpec, ReturnTypeRule};
#[cfg(test)]
use crate::functions::spec::ParamType;
use crate::types::DataType;

pub(super) fn specs() -> Vec<FunctionSpec> {
    vec![
        scalar(
            "abs",
            vec![sig(vec![p_numeric()])],
            ReturnTypeRule::SameAsFirstArg,
            "Type-preserving absolute value over Numeric.",
        ),
        scalar(
            "round",
            vec![
                sig(vec![p(DataType::Float)]),
                sig(vec![p(DataType::Float), p(DataType::Integer)]),
                sig(vec![p(DataType::Double)]),
                sig(vec![p(DataType::Double), p(DataType::Integer)]),
            ],
            ReturnTypeRule::SameAsFirstArg,
            "Half-away-from-zero rounding. Decimal overloads thread through SameAsFirstArg.",
        ),
        scalar(
            "ceil",
            vec![
                sig(vec![p(DataType::Float)]),
                sig(vec![p(DataType::Double)]),
            ],
            ReturnTypeRule::SameAsFirstArg,
            "Ceiling. v1 single-arg only.",
        ),
        scalar(
            "floor",
            vec![
                sig(vec![p(DataType::Float)]),
                sig(vec![p(DataType::Double)]),
            ],
            ReturnTypeRule::SameAsFirstArg,
            "Floor. v1 single-arg only.",
        ),
        scalar(
            "sqrt",
            vec![sig(vec![p(DataType::Double)])],
            ReturnTypeRule::Fixed(DataType::Double),
            "Square root.",
        ),
        scalar(
            "power",
            vec![sig(vec![p(DataType::Double), p(DataType::Double)])],
            ReturnTypeRule::Fixed(DataType::Double),
            "Exponentiation.",
        ),
        scalar(
            "exp",
            vec![sig(vec![p(DataType::Double)])],
            ReturnTypeRule::Fixed(DataType::Double),
            "Natural exponential.",
        ),
        scalar(
            "ln",
            vec![sig(vec![p(DataType::Double)])],
            ReturnTypeRule::Fixed(DataType::Double),
            "Natural logarithm.",
        ),
        scalar(
            "log",
            vec![sig(vec![p(DataType::Double), p(DataType::Double)])],
            ReturnTypeRule::Fixed(DataType::Double),
            "log(base, value); 1-arg form is NOT canonical (engines disagree).",
        ),
        scalar(
            "log10",
            vec![sig(vec![p(DataType::Double)])],
            ReturnTypeRule::Fixed(DataType::Double),
            "Base-10 logarithm.",
        ),
        scalar(
            "sign",
            vec![sig(vec![p_numeric()])],
            ReturnTypeRule::Fixed(DataType::Integer),
            "Returns -1, 0, 1; Integer for portability.",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn family_has_eleven_entries() {
        assert_eq!(specs().len(), 11);
    }

    #[test]
    fn abs_is_same_as_first_arg_over_numeric() {
        let v = specs();
        let abs = v.iter().find(|s| s.name.as_str() == "abs").unwrap();
        assert!(matches!(abs.return_type, ReturnTypeRule::SameAsFirstArg));
        assert_eq!(abs.signatures.len(), 1);
        assert!(matches!(
            abs.signatures[0].params[0],
            ParamType::NumericFamily
        ));
    }

    #[test]
    fn sign_returns_integer() {
        let v = specs();
        let sign = v.iter().find(|s| s.name.as_str() == "sign").unwrap();
        assert!(matches!(
            sign.return_type,
            ReturnTypeRule::Fixed(DataType::Integer)
        ));
    }

    #[test]
    fn round_has_four_overloads() {
        let v = specs();
        let r = v.iter().find(|s| s.name.as_str() == "round").unwrap();
        assert_eq!(r.signatures.len(), 4);
    }

    #[test]
    fn log_is_two_arg_only() {
        let v = specs();
        let log = v.iter().find(|s| s.name.as_str() == "log").unwrap();
        assert_eq!(log.signatures.len(), 1);
        assert_eq!(log.signatures[0].params.len(), 2);
    }

    #[test]
    fn sqrt_takes_one_double() {
        let v = specs();
        let sqrt = v.iter().find(|s| s.name.as_str() == "sqrt").unwrap();
        assert_eq!(sqrt.signatures.len(), 1);
        assert_eq!(sqrt.signatures[0].params.len(), 1);
        assert!(matches!(
            sqrt.return_type,
            ReturnTypeRule::Fixed(DataType::Double)
        ));
    }
}
