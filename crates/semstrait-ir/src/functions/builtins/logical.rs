//! Scalar — Logical / Conditional helpers. Per `14a §4.5`. 2 entries;
//! reserved AST variants (`Case`, `Coalesce`, `NullIf`, `IsNull`) are not
//! registry entries.
//!
//! `greatest` / `least` use `SameAsFirstArg` as a v1 approximation —
//! `Promoted(&'static [usize])` cannot enumerate variadic indices
//! statically. Compile-time promotion lives in the manifest pipeline; this
//! catalog only records resolution shape. Tracked as
//! `[TD-REGISTRY-VARIADIC-PROMOTED]`.

use crate::functions::builtins::dsl::{p_any, scalar, variadic};
use crate::functions::spec::{FunctionSpec, ReturnTypeRule};

pub(super) fn specs() -> Vec<FunctionSpec> {
    vec![
        scalar(
            "greatest",
            vec![variadic(vec![p_any()], p_any())],
            ReturnTypeRule::SameAsFirstArg,
        ),
        scalar(
            "least",
            vec![variadic(vec![p_any()], p_any())],
            ReturnTypeRule::SameAsFirstArg,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::functions::spec::FunctionCategory;

    #[test]
    fn family_has_two_entries() {
        assert_eq!(specs().len(), 2);
    }

    #[test]
    fn greatest_and_least_are_scalar_variadic() {
        let v = specs();
        for name in ["greatest", "least"] {
            let s = v.iter().find(|s| s.name.as_str() == name).unwrap();
            assert_eq!(s.category, FunctionCategory::Scalar);
            assert_eq!(s.signatures.len(), 1);
            assert!(s.signatures[0].variadic_tail.is_some());
        }
    }
}
