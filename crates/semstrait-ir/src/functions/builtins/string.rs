//! Scalar — String. Per `14a §4.2`.
//!
//! 12 entries. v1 omits the `(Array<T>) -> ...` overloads from `length`
//! / `reverse` because `DataType` has no `Array` variant yet — see
//! `13 §2.5` (complex types out of scope for v1).

use crate::functions::builtins::dsl::{
    p, p_string, scalar, sig, variadic,
};
use crate::functions::spec::{FunctionSpec, ReturnTypeRule};
use crate::types::DataType;

pub(super) fn specs() -> Vec<FunctionSpec> {
    vec![
        scalar(
            "upper",
            vec![sig(vec![p(DataType::String)])],
            ReturnTypeRule::Fixed(DataType::String),
            "Unicode-aware uppercase mapping.",
        ),
        scalar(
            "lower",
            vec![sig(vec![p(DataType::String)])],
            ReturnTypeRule::Fixed(DataType::String),
            "Unicode-aware lowercase mapping.",
        ),
        scalar(
            "length",
            vec![sig(vec![p(DataType::String)])],
            ReturnTypeRule::Fixed(DataType::Integer),
            "Character count of a String.",
        ),
        scalar(
            "substring",
            vec![
                sig(vec![p(DataType::String), p(DataType::Integer)]),
                sig(vec![
                    p(DataType::String),
                    p(DataType::Integer),
                    p(DataType::Integer),
                ]),
            ],
            ReturnTypeRule::Fixed(DataType::String),
            "1-indexed substring; positive position only in v1.",
        ),
        scalar(
            "trim",
            vec![
                sig(vec![p(DataType::String)]),
                sig(vec![p(DataType::String), p(DataType::String)]),
            ],
            ReturnTypeRule::Fixed(DataType::String),
            "Strip ASCII space (1-arg) or set semantics (2-arg).",
        ),
        scalar(
            "ltrim",
            vec![
                sig(vec![p(DataType::String)]),
                sig(vec![p(DataType::String), p(DataType::String)]),
            ],
            ReturnTypeRule::Fixed(DataType::String),
            "Left-strip ASCII space or set semantics.",
        ),
        scalar(
            "rtrim",
            vec![
                sig(vec![p(DataType::String)]),
                sig(vec![p(DataType::String), p(DataType::String)]),
            ],
            ReturnTypeRule::Fixed(DataType::String),
            "Right-strip ASCII space or set semantics.",
        ),
        scalar(
            "concat",
            vec![variadic(
                vec![p(DataType::String)],
                p_string(),
            )],
            ReturnTypeRule::Fixed(DataType::String),
            "Concatenate strings; NULL-propagating.",
        ),
        scalar(
            "replace",
            vec![sig(vec![
                p(DataType::String),
                p(DataType::String),
                p(DataType::String),
            ])],
            ReturnTypeRule::Fixed(DataType::String),
            "Replace all occurrences of a literal substring.",
        ),
        scalar(
            "lpad",
            vec![
                sig(vec![p(DataType::String), p(DataType::Integer)]),
                sig(vec![
                    p(DataType::String),
                    p(DataType::Integer),
                    p(DataType::String),
                ]),
            ],
            ReturnTypeRule::Fixed(DataType::String),
            "Left-pad to target length with ASCII space (2-arg) or supplied pad (3-arg).",
        ),
        scalar(
            "rpad",
            vec![
                sig(vec![p(DataType::String), p(DataType::Integer)]),
                sig(vec![
                    p(DataType::String),
                    p(DataType::Integer),
                    p(DataType::String),
                ]),
            ],
            ReturnTypeRule::Fixed(DataType::String),
            "Right-pad to target length with ASCII space (2-arg) or supplied pad (3-arg).",
        ),
        scalar(
            "reverse",
            vec![sig(vec![p(DataType::String)])],
            ReturnTypeRule::Fixed(DataType::String),
            "Reverse by code points.",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::functions::spec::{FunctionCategory, ParamType};

    #[test]
    fn family_has_twelve_entries() {
        assert_eq!(specs().len(), 12);
    }

    #[test]
    fn upper_and_lower_take_one_string_arg() {
        let v = specs();
        for name in ["upper", "lower"] {
            let spec = v.iter().find(|s| s.name.as_str() == name).unwrap();
            assert_eq!(spec.category, FunctionCategory::Scalar);
            assert_eq!(spec.signatures.len(), 1);
            assert_eq!(spec.signatures[0].params.len(), 1);
            assert!(matches!(
                &spec.signatures[0].params[0],
                ParamType::Concrete(DataType::String)
            ));
            assert!(matches!(
                spec.return_type,
                ReturnTypeRule::Fixed(DataType::String)
            ));
        }
    }

    #[test]
    fn length_returns_integer() {
        let v = specs();
        let length = v.iter().find(|s| s.name.as_str() == "length").unwrap();
        assert!(matches!(
            length.return_type,
            ReturnTypeRule::Fixed(DataType::Integer)
        ));
    }

    #[test]
    fn substring_has_two_overloads() {
        let v = specs();
        let s = v.iter().find(|s| s.name.as_str() == "substring").unwrap();
        assert_eq!(s.signatures.len(), 2);
        assert_eq!(s.signatures[0].params.len(), 2);
        assert_eq!(s.signatures[1].params.len(), 3);
    }

    #[test]
    fn concat_is_variadic_with_string_tail() {
        let v = specs();
        let c = v.iter().find(|s| s.name.as_str() == "concat").unwrap();
        assert_eq!(c.signatures.len(), 1);
        assert!(c.signatures[0].variadic_tail.is_some());
        match c.signatures[0].variadic_tail.as_ref().unwrap() {
            ParamType::StringFamily => (),
            other => panic!("expected StringFamily tail, got {other:?}"),
        }
    }

    #[test]
    fn no_string_entry_carries_additivity() {
        for spec in specs() {
            assert!(spec.additivity.is_none(), "{}", spec.name.as_str());
        }
    }
}
