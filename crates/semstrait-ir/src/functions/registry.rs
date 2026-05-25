//! Function registry — sealed singleton.
//!
//! Per spec `35 §8.2` / `14a §2`. The registry is built once via
//! `OnceLock` inside [`function_registry`]; immutable post-init. The
//! bootstrap step folds the canonical built-in catalog into a flat
//! `HashMap`, panicking on duplicate names or reserved-tag collisions.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::functions::builtins;
use crate::functions::canonical_fn::CanonicalFn;
use crate::functions::spec::FunctionSpec;

/// Reserved AST tag roster per `14 §6.4.1`. Registry entries colliding
/// with any of these surface as a startup panic per `14a §7.2`. v1
/// roster mirrors the structural / leaf tags ratified in `14`.
const RESERVED_AST_TAGS: &[&str] = &[
    "col",
    "literal",
    "field",
    "dim",
    "measure",
    "metric",
    "key",
    "cast",
    "case",
    "coalesce",
    "nullif",
    "is_null",
    "between",
    "in_list",
    "like",
    "aggregate",
    "window",
];

/// The single canonical function catalog. Per `35 §8.2` / `14a §2.2`.
///
/// Sealed at startup; immutable thereafter. Built once inside
/// [`function_registry`] via `OnceLock`.
pub struct FunctionRegistry {
    by_name: HashMap<String, FunctionSpec>,
}

impl FunctionRegistry {
    fn bootstrap() -> Self {
        let mut by_name: HashMap<String, FunctionSpec> = HashMap::new();
        for spec in builtins::assemble_core_specs() {
            assert!(
                !spec.signatures.is_empty(),
                "FunctionSpec `{}` declares no signatures",
                spec.name.as_str()
            );
            let key = spec.name.as_str().to_string();
            assert!(
                !RESERVED_AST_TAGS.contains(&key.as_str()),
                "registry entry `{key}` collides with reserved AST tag (per 14 §6.4.1)"
            );
            assert!(
                !by_name.contains_key(&key),
                "duplicate canonical function name `{key}` in built-in catalog"
            );
            by_name.insert(key, spec);
        }
        // Adapter extensions are deferred per [TD-REGISTRY-EXTENSION-WIRING];
        // no runtime enumeration of `RegistryExtension` impls in v1.
        Self { by_name }
    }

    /// Look up a `FunctionSpec` by canonical name. Per `35 §8.2`.
    pub fn lookup(&self, name: &CanonicalFn) -> Option<&FunctionSpec> {
        self.by_name.get(name.as_str())
    }

    /// Membership predicate. Per `35 §8.2`.
    pub fn contains(&self, name: &CanonicalFn) -> bool {
        self.by_name.contains_key(name.as_str())
    }

    /// Yield every `(name, spec)` pair. Per `35 §8.2`.
    ///
    /// Names are borrowed from the spec's own `name` field, avoiding
    /// throwaway `CanonicalFn` allocations.
    pub fn iter(&self) -> impl Iterator<Item = (&CanonicalFn, &FunctionSpec)> + '_ {
        self.by_name.values().map(|spec| (&spec.name, spec))
    }

    /// Per `35 §8.2`: the registry is sealed post-init. Always `true`
    /// — present for API symmetry with future Building-state extensions.
    pub fn is_sealed(&self) -> bool {
        true
    }
}

/// Process-wide singleton accessor. Per `35 §8.2` / `14a §2.1`.
pub fn function_registry() -> &'static FunctionRegistry {
    static REGISTRY: OnceLock<FunctionRegistry> = OnceLock::new();
    REGISTRY.get_or_init(FunctionRegistry::bootstrap)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::functions::spec::{Additivity, FunctionCategory};

    #[test]
    fn function_registry_returns_singleton() {
        let a = function_registry();
        let b = function_registry();
        assert!(std::ptr::eq(a, b));
    }

    #[test]
    fn lookup_known_string_function() {
        let r = function_registry();
        let upper = CanonicalFn::new("upper").unwrap();
        let spec = r.lookup(&upper).expect("upper must be registered");
        assert_eq!(spec.category, FunctionCategory::Scalar);
        assert!(!spec.signatures.is_empty());
    }

    #[test]
    fn lookup_unknown_returns_none() {
        let r = function_registry();
        let bogus = CanonicalFn::new("not_a_real_fn_xyz").unwrap();
        assert!(r.lookup(&bogus).is_none());
    }

    #[test]
    fn contains_matches_lookup() {
        let r = function_registry();
        let upper = CanonicalFn::new("upper").unwrap();
        let bogus = CanonicalFn::new("not_a_real_fn_xyz").unwrap();
        assert_eq!(r.contains(&upper), r.lookup(&upper).is_some());
        assert!(!r.contains(&bogus));
    }

    #[test]
    fn lookup_case_insensitive_via_canonical_fn() {
        let r = function_registry();
        let lower = CanonicalFn::new("upper").unwrap();
        let upper_input = CanonicalFn::new("UPPER").unwrap();
        // Both normalize to "upper"; both must resolve to the same spec.
        let a = r.lookup(&lower).unwrap();
        let b = r.lookup(&upper_input).unwrap();
        assert_eq!(a.name, b.name);
    }

    #[test]
    fn iter_yields_full_catalog() {
        let r = function_registry();
        // 12 string + 11 math + 14 temporal + 2 logical + 8 aggregate = 47.
        assert_eq!(r.iter().count(), 47);
    }

    #[test]
    fn iter_pairs_match_individual_lookups() {
        let r = function_registry();
        for (name, spec) in r.iter() {
            let resolved = r.lookup(name).expect("iter name must resolve");
            assert_eq!(resolved.name, spec.name);
        }
    }

    #[test]
    fn is_sealed_returns_true_post_init() {
        assert!(function_registry().is_sealed());
    }

    #[test]
    fn aggregate_entries_carry_non_additive() {
        let r = function_registry();
        let median = CanonicalFn::new("median").unwrap();
        let spec = r.lookup(&median).unwrap();
        assert_eq!(spec.category, FunctionCategory::Aggregate);
        assert_eq!(spec.additivity, Some(Additivity::NonAdditive));
    }

    #[test]
    fn scalar_entries_carry_no_additivity() {
        let r = function_registry();
        let upper = CanonicalFn::new("upper").unwrap();
        let spec = r.lookup(&upper).unwrap();
        assert_eq!(spec.category, FunctionCategory::Scalar);
        assert!(spec.additivity.is_none());
    }

    #[test]
    fn no_registered_name_collides_with_reserved_tag() {
        let r = function_registry();
        for (name, _) in r.iter() {
            assert!(
                !RESERVED_AST_TAGS.contains(&name.as_str()),
                "registered entry `{}` collides with reserved AST tag",
                name.as_str()
            );
        }
    }
}
