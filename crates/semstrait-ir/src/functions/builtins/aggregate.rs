//! Aggregate — non-closed. Per `14a §4.6`. 8 entries; closed-five live on
//! `Expr::Aggregate` per `14 §3.3`.

use crate::functions::builtins::dsl::{aggregate, p, p_any, p_decimal, p_numeric, sig};
use crate::functions::spec::{Additivity, FunctionSpec, ReturnTypeRule};
use crate::types::DataType;

pub(super) fn specs() -> Vec<FunctionSpec> {
    vec![
        aggregate(
            "stddev",
            vec![sig(vec![p_numeric()])],
            ReturnTypeRule::Fixed(DataType::Double),
            Additivity::NonAdditive,
        ),
        aggregate(
            "stddev_pop",
            vec![sig(vec![p_numeric()])],
            ReturnTypeRule::Fixed(DataType::Double),
            Additivity::NonAdditive,
        ),
        aggregate(
            "variance",
            vec![sig(vec![p_numeric()])],
            ReturnTypeRule::Fixed(DataType::Double),
            Additivity::NonAdditive,
        ),
        aggregate(
            "var_pop",
            vec![sig(vec![p_numeric()])],
            ReturnTypeRule::Fixed(DataType::Double),
            Additivity::NonAdditive,
        ),
        aggregate(
            "median",
            vec![sig(vec![p_numeric()]), sig(vec![p_decimal()])],
            ReturnTypeRule::SameAsFirstArg,
            Additivity::NonAdditive,
        ),
        aggregate(
            "string_agg",
            vec![sig(vec![p(DataType::String), p(DataType::String)])],
            ReturnTypeRule::Fixed(DataType::String),
            Additivity::NonAdditive,
        ),
        aggregate(
            "percentile_cont",
            vec![sig(vec![p(DataType::Double), p_numeric()])],
            ReturnTypeRule::Fixed(DataType::Double),
            Additivity::NonAdditive,
        ),
        aggregate(
            "approx_count_distinct",
            vec![sig(vec![p_any()])],
            ReturnTypeRule::Fixed(DataType::Long),
            Additivity::NonAdditive,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::functions::spec::{FunctionCategory, ParamType};

    #[test]
    fn family_has_eight_entries() {
        assert_eq!(specs().len(), 8);
    }

    #[test]
    fn every_entry_is_aggregate_non_additive() {
        for spec in specs() {
            assert_eq!(spec.category, FunctionCategory::Aggregate);
            assert!(matches!(spec.additivity, Some(Additivity::NonAdditive)));
        }
    }

    #[test]
    fn string_agg_takes_two_strings_returning_string() {
        let v = specs();
        let sa = v.iter().find(|s| s.name.as_str() == "string_agg").unwrap();
        assert_eq!(sa.signatures.len(), 1);
        assert_eq!(sa.signatures[0].params.len(), 2);
        assert!(matches!(
            sa.return_type,
            ReturnTypeRule::Fixed(DataType::String)
        ));
    }

    #[test]
    fn approx_count_distinct_returns_long() {
        let v = specs();
        let acd = v
            .iter()
            .find(|s| s.name.as_str() == "approx_count_distinct")
            .unwrap();
        assert!(matches!(
            acd.return_type,
            ReturnTypeRule::Fixed(DataType::Long)
        ));
    }

    #[test]
    fn median_threads_decimal_via_same_as_first_arg() {
        let v = specs();
        let m = v.iter().find(|s| s.name.as_str() == "median").unwrap();
        assert!(matches!(m.return_type, ReturnTypeRule::SameAsFirstArg));
        assert_eq!(m.signatures.len(), 2);
        assert!(matches!(
            m.signatures[1].params[0],
            ParamType::DecimalFamily
        ));
    }
}
