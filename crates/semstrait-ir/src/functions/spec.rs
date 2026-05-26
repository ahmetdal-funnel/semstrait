//! Function-spec types per `35 §8.2` and `14a §3`.

use crate::error::CompileError;
use crate::functions::canonical_fn::CanonicalFn;
use crate::types::DataType;

/// Per-function specification — `35 §8.2` / `14a §3`.
///
/// `PartialEq` intentionally not derived: `ReturnTypeRule::Custom` carries
/// a fn-pointer that cannot be compared honestly. Callers compare
/// individual fields.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct FunctionSpec {
    pub name: CanonicalFn,
    pub category: FunctionCategory,
    pub signatures: Vec<FnSignature>,
    pub return_type: ReturnTypeRule,
    pub additivity: Option<Additivity>,
}

/// One signature overload — argument types + arity discipline.
/// Per `14a §3.3` / `35 §8.2`. `variadic_tail = Some(T)` accepts
/// zero-or-more trailing args of type `T` after the fixed `params`.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub struct FnSignature {
    pub params: Vec<ParamType>,
    pub variadic_tail: Option<ParamType>,
}

/// Parameter-type carrier. Per `14a §3.5` / `35 §8.2`.
///
/// Family variants ground in [`crate::types::TypeClass`]; full
/// type-class generics are deferred per `[TD-REGISTRY-TYPECLASS]`.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum ParamType {
    Concrete(DataType),
    AnyOf(Vec<DataType>),
    NumericFamily,
    StringFamily,
    TemporalFamily,
    /// Any `DataType::Decimal { .. }`. Used by signatures that thread
    /// `(precision, scale)` through `SameAsFirstArg` / `DecimalScaleZero`.
    DecimalFamily,
    Any,
}

/// Return-type computation rule. Per `35 §8.2` / `14a §3.4`.
///
/// Resolution lives downstream in the manifest compile pipeline; this
/// module emits the rule and doesn't evaluate it. `PartialEq` is not
/// derived: `Custom` carries an unstable fn-pointer.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum ReturnTypeRule {
    Fixed(DataType),
    SameAsFirstArg,
    SameAsArg(u32),
    Promoted(&'static [usize]),
    /// `Decimal(p, s)` → `Decimal(p, 0)`. For `ceil` / `floor`.
    DecimalScaleZero,
    Custom(fn(&[DataType]) -> Result<DataType, CompileError>),
}

/// Function-category axis. Per `35 §8.2` / `14a §3.2`.
///
/// `Window` is carried for API parity per spec 35; per `14a §3.2`,
/// `Window` is compile-emitted only (sugar-accessor elimination per
/// `14 §4.2`) and is NOT author-registered in v1's canonical catalog.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FunctionCategory {
    Scalar,
    Aggregate,
    Window,
}

/// Function-level additivity. Per `14a §3.6`.
///
/// `None` for scalar functions. Composed with model-level
/// `AdditivityType` per `19 §6.5`'s effective-additivity rule.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum Additivity {
    Additive,
    SemiAdditive { axes: Vec<DimensionAxis> },
    NonAdditive,
}

/// Axis along which a `SemiAdditive` aggregate is non-summable.
/// Per `14a §3.6`. v1 only enumerates `Temporal`; future axes may
/// follow.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DimensionAxis {
    Temporal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn return_type_rule_custom_invokes_closure() {
        fn rule(_: &[DataType]) -> Result<DataType, CompileError> {
            Ok(DataType::Long)
        }
        let r = ReturnTypeRule::Custom(rule);
        if let ReturnTypeRule::Custom(f) = r {
            let out = f(&[DataType::Integer]).unwrap();
            assert_eq!(out, DataType::Long);
        } else {
            panic!("expected Custom");
        }
    }

    #[test]
    fn return_type_rule_custom_propagates_error() {
        fn rule(_: &[DataType]) -> Result<DataType, CompileError> {
            Err(CompileError::CustomRuleRejected {
                fn_name: CanonicalFn::new("dummy").unwrap(),
                args: vec![],
                reason: "test".to_string(),
            })
        }
        let r = ReturnTypeRule::Custom(rule);
        if let ReturnTypeRule::Custom(f) = r {
            assert!(f(&[]).is_err());
        }
    }

    #[test]
    fn fn_signature_holds_params_and_optional_variadic() {
        let s = FnSignature {
            params: vec![ParamType::Concrete(DataType::String)],
            variadic_tail: Some(ParamType::Concrete(DataType::String)),
        };
        assert_eq!(s.params.len(), 1);
        assert!(s.variadic_tail.is_some());
    }
}
