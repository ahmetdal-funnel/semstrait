//! Scalar — Temporal. Per `14a §4.4`. 14 entries; `EXTRACT` lowers to
//! `date_part` (parser sugar, not a registry entry).

use crate::functions::builtins::dsl::{p, scalar, sig};
use crate::functions::spec::{FunctionSpec, ReturnTypeRule};
use crate::types::DataType;

pub(super) fn specs() -> Vec<FunctionSpec> {
    let ts_default = DataType::Timestamp { precision: 6 };

    vec![
        scalar(
            "date_part",
            vec![
                sig(vec![p(DataType::String), p(DataType::Date)]),
                sig(vec![p(DataType::String), p(ts_default.clone())]),
            ],
            ReturnTypeRule::Fixed(DataType::Long),
        ),
        scalar(
            "year",
            vec![
                sig(vec![p(DataType::Date)]),
                sig(vec![p(ts_default.clone())]),
            ],
            ReturnTypeRule::Fixed(DataType::Long),
        ),
        scalar(
            "month",
            vec![
                sig(vec![p(DataType::Date)]),
                sig(vec![p(ts_default.clone())]),
            ],
            ReturnTypeRule::Fixed(DataType::Long),
        ),
        scalar(
            "day",
            vec![
                sig(vec![p(DataType::Date)]),
                sig(vec![p(ts_default.clone())]),
            ],
            ReturnTypeRule::Fixed(DataType::Long),
        ),
        scalar(
            "hour",
            vec![sig(vec![p(ts_default.clone())])],
            ReturnTypeRule::Fixed(DataType::Long),
        ),
        scalar(
            "minute",
            vec![sig(vec![p(ts_default.clone())])],
            ReturnTypeRule::Fixed(DataType::Long),
        ),
        scalar(
            "second",
            vec![sig(vec![p(ts_default.clone())])],
            ReturnTypeRule::Fixed(DataType::Long),
        ),
        scalar(
            "date_add",
            vec![
                sig(vec![p(DataType::Date), p(DataType::Interval)]),
                sig(vec![p(ts_default.clone()), p(DataType::Interval)]),
            ],
            ReturnTypeRule::SameAsFirstArg,
        ),
        scalar(
            "date_sub",
            vec![
                sig(vec![p(DataType::Date), p(DataType::Interval)]),
                sig(vec![p(ts_default.clone()), p(DataType::Interval)]),
            ],
            ReturnTypeRule::SameAsFirstArg,
        ),
        scalar(
            "date_diff",
            vec![
                sig(vec![
                    p(DataType::String),
                    p(DataType::Date),
                    p(DataType::Date),
                ]),
                sig(vec![
                    p(DataType::String),
                    p(ts_default.clone()),
                    p(ts_default.clone()),
                ]),
            ],
            ReturnTypeRule::Fixed(DataType::Long),
        ),
        scalar(
            "to_date",
            vec![sig(vec![p(DataType::String)])],
            ReturnTypeRule::Fixed(DataType::Date),
        ),
        scalar(
            "to_timestamp",
            vec![sig(vec![p(DataType::String)])],
            ReturnTypeRule::Fixed(ts_default.clone()),
        ),
        scalar(
            "current_date",
            vec![sig(vec![])],
            ReturnTypeRule::Fixed(DataType::Date),
        ),
        scalar(
            "current_timestamp",
            vec![sig(vec![])],
            ReturnTypeRule::Fixed(ts_default),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn family_has_fourteen_entries() {
        assert_eq!(specs().len(), 14);
    }

    #[test]
    fn current_date_is_zero_arg_returning_date() {
        let v = specs();
        let cd = v
            .iter()
            .find(|s| s.name.as_str() == "current_date")
            .unwrap();
        assert_eq!(cd.signatures.len(), 1);
        assert_eq!(cd.signatures[0].params.len(), 0);
        assert!(matches!(
            cd.return_type,
            ReturnTypeRule::Fixed(DataType::Date)
        ));
    }

    #[test]
    fn current_timestamp_returns_timestamp() {
        let v = specs();
        let ct = v
            .iter()
            .find(|s| s.name.as_str() == "current_timestamp")
            .unwrap();
        assert!(matches!(
            ct.return_type,
            ReturnTypeRule::Fixed(DataType::Timestamp { .. })
        ));
    }

    #[test]
    fn year_has_two_overloads_returning_long() {
        let v = specs();
        let y = v.iter().find(|s| s.name.as_str() == "year").unwrap();
        assert_eq!(y.signatures.len(), 2);
        assert!(matches!(
            y.return_type,
            ReturnTypeRule::Fixed(DataType::Long)
        ));
    }

    #[test]
    fn date_add_threads_first_arg_type() {
        let v = specs();
        let da = v.iter().find(|s| s.name.as_str() == "date_add").unwrap();
        assert!(matches!(da.return_type, ReturnTypeRule::SameAsFirstArg));
        assert_eq!(da.signatures.len(), 2);
    }

    #[test]
    fn date_part_first_arg_is_string() {
        let v = specs();
        let dp = v.iter().find(|s| s.name.as_str() == "date_part").unwrap();
        for sig in &dp.signatures {
            assert!(matches!(
                &sig.params[0],
                crate::functions::spec::ParamType::Concrete(DataType::String)
            ));
        }
    }
}
