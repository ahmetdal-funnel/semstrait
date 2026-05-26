//! Scalar — Math. Per `14a §4.3`. 11 entries; `mod` is `BinaryOpKind::Mod`.

use crate::functions::builtins::dsl::{p, p_decimal, p_numeric, scalar, sig};
#[cfg(test)]
use crate::functions::spec::ParamType;
use crate::functions::spec::{FunctionSpec, ReturnTypeRule};
use crate::types::DataType;

pub(super) fn specs() -> Vec<FunctionSpec> {
    vec![
        scalar(
            "abs",
            vec![sig(vec![p_numeric()])],
            ReturnTypeRule::SameAsFirstArg,
        ),
        scalar(
            "round",
            vec![
                sig(vec![p(DataType::Float)]),
                sig(vec![p(DataType::Float), p(DataType::Integer)]),
                sig(vec![p(DataType::Double)]),
                sig(vec![p(DataType::Double), p(DataType::Integer)]),
                sig(vec![p_decimal()]),
                sig(vec![p_decimal(), p(DataType::Integer)]),
            ],
            ReturnTypeRule::SameAsFirstArg,
        ),
        scalar(
            "ceil",
            vec![
                sig(vec![p(DataType::Float)]),
                sig(vec![p(DataType::Double)]),
                sig(vec![p_decimal()]),
            ],
            ReturnTypeRule::DecimalScaleZero,
        ),
        scalar(
            "floor",
            vec![
                sig(vec![p(DataType::Float)]),
                sig(vec![p(DataType::Double)]),
                sig(vec![p_decimal()]),
            ],
            ReturnTypeRule::DecimalScaleZero,
        ),
        scalar(
            "sqrt",
            vec![sig(vec![p(DataType::Double)])],
            ReturnTypeRule::Fixed(DataType::Double),
        ),
        scalar(
            "power",
            vec![sig(vec![p(DataType::Double), p(DataType::Double)])],
            ReturnTypeRule::Fixed(DataType::Double),
        ),
        scalar(
            "exp",
            vec![sig(vec![p(DataType::Double)])],
            ReturnTypeRule::Fixed(DataType::Double),
        ),
        scalar(
            "ln",
            vec![sig(vec![p(DataType::Double)])],
            ReturnTypeRule::Fixed(DataType::Double),
        ),
        scalar(
            "log",
            vec![sig(vec![p(DataType::Double), p(DataType::Double)])],
            ReturnTypeRule::Fixed(DataType::Double),
        ),
        scalar(
            "log10",
            vec![sig(vec![p(DataType::Double)])],
            ReturnTypeRule::Fixed(DataType::Double),
        ),
        scalar(
            "sign",
            vec![sig(vec![p_numeric()])],
            ReturnTypeRule::Fixed(DataType::Integer),
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
    fn round_has_six_overloads_including_decimal() {
        let v = specs();
        let r = v.iter().find(|s| s.name.as_str() == "round").unwrap();
        assert_eq!(r.signatures.len(), 6);
        // last two overloads thread Decimal through SameAsFirstArg
        assert!(matches!(r.return_type, ReturnTypeRule::SameAsFirstArg));
        assert!(matches!(
            r.signatures[4].params[0],
            ParamType::DecimalFamily
        ));
        assert!(matches!(
            r.signatures[5].params[0],
            ParamType::DecimalFamily
        ));
    }

    #[test]
    fn ceil_and_floor_use_decimal_scale_zero() {
        let v = specs();
        for name in ["ceil", "floor"] {
            let s = v.iter().find(|s| s.name.as_str() == name).unwrap();
            assert!(matches!(s.return_type, ReturnTypeRule::DecimalScaleZero));
            assert_eq!(s.signatures.len(), 3);
            assert!(matches!(
                s.signatures[2].params[0],
                ParamType::DecimalFamily
            ));
        }
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
