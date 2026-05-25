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

use crate::functions::spec::FunctionSpec;

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
        description: &'static str,
    ) -> FunctionSpec {
        FunctionSpec {
            name: CanonicalFn::new(name).expect("valid catalog name"),
            category: FunctionCategory::Scalar,
            signatures,
            return_type,
            additivity: None,
            description,
        }
    }

    pub fn aggregate(
        name: &str,
        signatures: Vec<FnSignature>,
        return_type: ReturnTypeRule,
        additivity: Additivity,
        description: &'static str,
    ) -> FunctionSpec {
        FunctionSpec {
            name: CanonicalFn::new(name).expect("valid catalog name"),
            category: FunctionCategory::Aggregate,
            signatures,
            return_type,
            additivity: Some(additivity),
            description,
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
}
