//! Scalar — Logical / Conditional helpers. Per `14a §4.5`.
//!
//! 2 entries. Reserved AST variants (`Case`, `Coalesce`, `NullIf`,
//! `IsNull`) are NOT registry entries.
//!
//! Open point per the implementation plan: `greatest`/`least` are
//! variadic with arbitrary-arity common-supertype promotion. The
//! `ReturnTypeRule::Promoted(&'static [usize])` variant cannot
//! enumerate variadic indices statically, so we record
//! [`ReturnTypeRule::SameAsFirstArg`] here as the v1 approximation —
//! every overload arg must share a comparable common type per spec.
//! Compile-time promotion logic lives in the manifest pipeline; this
//! catalog only carries the resolution shape. Tracked under
//! `[TD-REGISTRY-VARIADIC-PROMOTED]`.

use crate::functions::builtins::dsl::{p_any, scalar, variadic};
use crate::functions::spec::{FunctionSpec, ReturnTypeRule};

pub(super) fn specs() -> Vec<FunctionSpec> {
    vec![
        scalar(
            "greatest",
            vec![variadic(vec![p_any()], p_any())],
            ReturnTypeRule::SameAsFirstArg,
            "Greatest non-NULL value; args must share a comparable common type.",
        ),
        scalar(
            "least",
            vec![variadic(vec![p_any()], p_any())],
            ReturnTypeRule::SameAsFirstArg,
            "Least non-NULL value; args must share a comparable common type.",
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
