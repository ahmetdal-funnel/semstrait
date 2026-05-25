//! Adapter / downstream-crate extension hook. Per `14a §7` / `35 §8.2`.

use crate::functions::spec::FunctionSpec;

/// Adapter / downstream-crate extension hook.
///
/// Per `14a §7`: implementers expose a static spec list plus an adapter
/// identity. `function_registry()` is intended to enumerate linked
/// extensions at startup and fold their `FUNCTIONS` into the sealed
/// registry.
///
/// **v1 wiring posture.** No automatic discovery in v1. The registry's
/// bootstrap does NOT enumerate impls (no `inventory`, `linkme`, or
/// build-script aggregation). The trait shape exists so adapter crates
/// can declare their extensions today; downstream wiring is tracked
/// under `[TD-REGISTRY-EXTENSION-WIRING]`.
///
/// Not sealed — adapter crates outside the workspace MAY contribute.
pub trait RegistryExtension {
    const ADAPTER_ID: &'static str;
    const FUNCTIONS: &'static [FunctionSpec];
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::functions::canonical_fn::CanonicalFn;
    use crate::functions::spec::{FnSignature, FunctionCategory, ParamType, ReturnTypeRule};
    use crate::types::DataType;

    struct DummyAdapter;

    impl RegistryExtension for DummyAdapter {
        const ADAPTER_ID: &'static str = "dummy";
        const FUNCTIONS: &'static [FunctionSpec] = &[];
    }

    #[test]
    fn trait_const_shapes_compile() {
        assert_eq!(DummyAdapter::ADAPTER_ID, "dummy");
        assert!(DummyAdapter::FUNCTIONS.is_empty());
    }

    #[test]
    fn extension_can_declare_a_function() {
        // We cannot construct CanonicalFn at compile time (its `new`
        // fn is non-const), so a real adapter would build a `static`
        // FunctionSpec via `OnceLock` or runtime registration. This
        // test just confirms the trait shape lets callers point at a
        // runtime-built slice.
        let _spec_runtime = FunctionSpec {
            name: CanonicalFn::new("sample").unwrap(),
            category: FunctionCategory::Scalar,
            signatures: vec![FnSignature {
                params: vec![ParamType::Concrete(DataType::Integer)],
                variadic_tail: None,
            }],
            return_type: ReturnTypeRule::SameAsFirstArg,
            additivity: None,
            description: "sample",
        };
    }
}
