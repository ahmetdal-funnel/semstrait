//! Aggregate — non-closed. Per `14a §4.6`.
//!
//! 8 entries. The closed-five aggregates (Sum/Avg/Count/Min/Max) live
//! on `Expr::Aggregate` per `14 §3.3` and are NOT registry entries.

use crate::functions::builtins::dsl::{aggregate, p, p_any, p_numeric, sig};
use crate::functions::spec::{Additivity, FunctionSpec, ReturnTypeRule};
use crate::types::DataType;

pub(super) fn specs() -> Vec<FunctionSpec> {
    vec![
        aggregate(
            "stddev",
            vec![sig(vec![p_numeric()])],
            ReturnTypeRule::Fixed(DataType::Double),
            Additivity::NonAdditive,
            "Sample standard deviation; bare name is sample across engines.",
        ),
        aggregate(
            "stddev_pop",
            vec![sig(vec![p_numeric()])],
            ReturnTypeRule::Fixed(DataType::Double),
            Additivity::NonAdditive,
            "Population standard deviation.",
        ),
        aggregate(
            "variance",
            vec![sig(vec![p_numeric()])],
            ReturnTypeRule::Fixed(DataType::Double),
            Additivity::NonAdditive,
            "Sample variance; bare name is sample across engines.",
        ),
        aggregate(
            "var_pop",
            vec![sig(vec![p_numeric()])],
            ReturnTypeRule::Fixed(DataType::Double),
            Additivity::NonAdditive,
            "Population variance.",
        ),
        aggregate(
            "median",
            vec![sig(vec![p_numeric()])],
            ReturnTypeRule::Fixed(DataType::Double),
            Additivity::NonAdditive,
            "Exact median.",
        ),
        aggregate(
            "string_agg",
            vec![sig(vec![p(DataType::String), p(DataType::String)])],
            ReturnTypeRule::Fixed(DataType::String),
            Additivity::NonAdditive,
            "Concatenate non-NULL values with separator.",
        ),
        aggregate(
            "percentile_cont",
            vec![sig(vec![p(DataType::Double), p_numeric()])],
            ReturnTypeRule::Fixed(DataType::Double),
            Additivity::NonAdditive,
            "Continuous percentile; first arg is fraction in [0.0, 1.0].",
        ),
        aggregate(
            "approx_count_distinct",
            vec![sig(vec![p_any()])],
            ReturnTypeRule::Fixed(DataType::Long),
            Additivity::NonAdditive,
            "HyperLogLog-class approximate distinct count.",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::functions::spec::FunctionCategory;

    #[test]
    fn family_has_eight_entries() {
        assert_eq!(specs().len(), 8);
    }

    #[test]
    fn every_entry_is_aggregate_non_additive() {
        for spec in specs() {
            assert_eq!(spec.category, FunctionCategory::Aggregate);
            assert_eq!(spec.additivity, Some(Additivity::NonAdditive));
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
}
