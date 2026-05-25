//! Function-spec types per `35 §8.2` and `14a §3`.

use crate::error::CompileError;
use crate::functions::canonical_fn::CanonicalFn;
use crate::types::DataType;

/// Per-function specification — name, signature overloads, category,
/// return-type rule, function-level additivity, and a short description.
///
/// Reconciliation: the `name` / `signatures` / `category` / `return_type`
/// fields come from spec `35 §8.2`. The `additivity` / `description`
/// fields come from `14a §3.1` / `§3.6` (foundations-mandated; spec
/// 35's omission is editorial).
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionSpec {
    pub name: CanonicalFn,
    pub category: FunctionCategory,
    pub signatures: Vec<FnSignature>,
    pub return_type: ReturnTypeRule,
    pub additivity: Option<Additivity>,
    pub description: &'static str,
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
    Any,
}

/// Return-type computation rule. Per `35 §8.2` / `14a §3.4`.
///
/// `Promoted` is carried per `14a §3.4` (common-supertype promotion of
/// the listed arg indices per `13 §2.6`); spec 35 omits it editorially.
/// Resolution of these rules lives downstream in the manifest compile
/// pipeline — this module emits the rule, doesn't evaluate it.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum ReturnTypeRule {
    Fixed(DataType),
    SameAsFirstArg,
    SameAsArg(u32),
    Promoted(&'static [usize]),
    Custom(fn(&[DataType]) -> Result<DataType, CompileError>),
}

// Manual `PartialEq` — `Custom` carries an `fn` pointer whose address
// is not reliably comparable across codegen units. Two `Custom`
// variants always compare unequal; treat them as opaque for derive-eq
// purposes.
impl PartialEq for ReturnTypeRule {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Fixed(a), Self::Fixed(b)) => a == b,
            (Self::SameAsFirstArg, Self::SameAsFirstArg) => true,
            (Self::SameAsArg(a), Self::SameAsArg(b)) => a == b,
            (Self::Promoted(a), Self::Promoted(b)) => a == b,
            (Self::Custom(_), Self::Custom(_)) => false,
            _ => false,
        }
    }
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
    fn param_type_variants_round_trip_via_clone() {
        let cases = vec![
            ParamType::Concrete(DataType::Integer),
            ParamType::AnyOf(vec![DataType::Integer, DataType::Long]),
            ParamType::NumericFamily,
            ParamType::StringFamily,
            ParamType::TemporalFamily,
            ParamType::Any,
        ];
        for p in cases {
            assert_eq!(p.clone(), p);
        }
    }

    #[test]
    fn return_type_rule_fixed_carries_data_type() {
        let r = ReturnTypeRule::Fixed(DataType::Boolean);
        let dbg = format!("{r:?}");
        assert!(dbg.contains("Fixed"));
        assert!(dbg.contains("Boolean"));
    }

    #[test]
    fn return_type_rule_same_as_first_arg_distinct() {
        let r = ReturnTypeRule::SameAsFirstArg;
        assert_ne!(
            format!("{r:?}"),
            format!("{:?}", ReturnTypeRule::Fixed(DataType::Integer))
        );
    }

    #[test]
    fn return_type_rule_same_as_arg_indexed() {
        let r = ReturnTypeRule::SameAsArg(2);
        let dbg = format!("{r:?}");
        assert!(dbg.contains("SameAsArg"));
        assert!(dbg.contains('2'));
    }

    #[test]
    fn return_type_rule_promoted_holds_indices() {
        const IDX: &[usize] = &[0, 1];
        let r = ReturnTypeRule::Promoted(IDX);
        let dbg = format!("{r:?}");
        assert!(dbg.contains("Promoted"));
        assert!(dbg.contains('0'));
        assert!(dbg.contains('1'));
    }

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
    fn function_category_three_variants_distinct() {
        assert_ne!(FunctionCategory::Scalar, FunctionCategory::Aggregate);
        assert_ne!(FunctionCategory::Scalar, FunctionCategory::Window);
        assert_ne!(FunctionCategory::Aggregate, FunctionCategory::Window);
    }

    #[test]
    fn additivity_semi_additive_carries_axes() {
        let a = Additivity::SemiAdditive {
            axes: vec![DimensionAxis::Temporal],
        };
        let b = a.clone();
        assert_eq!(a, b);
        let c = Additivity::Additive;
        assert_ne!(a, c);
    }

    #[test]
    fn dimension_axis_temporal_present() {
        let t = DimensionAxis::Temporal;
        assert_eq!(t, DimensionAxis::Temporal);
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

    #[test]
    fn function_spec_debug_includes_name() {
        let spec = FunctionSpec {
            name: CanonicalFn::new("upper").unwrap(),
            category: FunctionCategory::Scalar,
            signatures: vec![FnSignature {
                params: vec![ParamType::Concrete(DataType::String)],
                variadic_tail: None,
            }],
            return_type: ReturnTypeRule::Fixed(DataType::String),
            additivity: None,
            description: "uppercase",
        };
        let dbg = format!("{spec:?}");
        assert!(dbg.contains("upper"));
    }
}
