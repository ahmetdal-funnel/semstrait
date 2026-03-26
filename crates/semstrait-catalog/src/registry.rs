//! Named catalog provider registry.
//!
//! Holds multiple [`CatalogProvider`](super::CatalogProvider) instances keyed by
//! user-chosen aliases from `catalogs.yaml`. Entities reference catalogs by alias
//! via [`CatalogRef`](semstrait_model::CatalogRef).

use std::collections::HashMap;
use std::sync::Arc;

use crate::CatalogProvider;

/// Registry of named catalog providers built from `CatalogsConfig`.
///
/// Supports multiple catalogs of the same provider type (e.g., `polaris_prod`
/// and `polaris_dev` both using Polaris). Used by `resolve_sources` to look up
/// the correct provider for each entity's `CatalogRef.alias`.
#[derive(Default)]
pub struct CatalogRegistry {
    providers: HashMap<String, Arc<dyn CatalogProvider>>,
}

impl std::fmt::Debug for CatalogRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CatalogRegistry")
            .field("aliases", &self.providers.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl CatalogRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a catalog provider under the given alias.
    ///
    /// Overwrites any existing provider with the same alias.
    pub fn register(&mut self, alias: impl Into<String>, provider: Arc<dyn CatalogProvider>) {
        self.providers.insert(alias.into(), provider);
    }

    /// Look up a catalog provider by alias.
    pub fn get(&self, alias: &str) -> Option<&Arc<dyn CatalogProvider>> {
        self.providers.get(alias)
    }

    /// Returns an iterator over registered alias names.
    pub fn aliases(&self) -> impl Iterator<Item = &str> {
        self.providers.keys().map(|s| s.as_str())
    }

    /// Returns the number of registered providers.
    pub fn len(&self) -> usize {
        self.providers.len()
    }

    /// Returns true if no providers are registered.
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NullCatalogProvider;

    #[test]
    fn test_empty_registry() {
        let registry = CatalogRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_register_and_get() {
        let mut registry = CatalogRegistry::new();
        registry.register("polaris_prod", Arc::new(NullCatalogProvider));
        registry.register("polaris_dev", Arc::new(NullCatalogProvider));

        assert_eq!(registry.len(), 2);
        assert!(!registry.is_empty());
        assert!(registry.get("polaris_prod").is_some());
        assert!(registry.get("polaris_dev").is_some());
        assert!(registry.get("missing").is_none());
    }

    #[test]
    fn test_aliases() {
        let mut registry = CatalogRegistry::new();
        registry.register("alpha", Arc::new(NullCatalogProvider));
        registry.register("beta", Arc::new(NullCatalogProvider));

        let mut aliases: Vec<&str> = registry.aliases().collect();
        aliases.sort();
        assert_eq!(aliases, vec!["alpha", "beta"]);
    }

    #[test]
    fn test_overwrite() {
        let mut registry = CatalogRegistry::new();
        registry.register("prod", Arc::new(NullCatalogProvider));
        registry.register("prod", Arc::new(NullCatalogProvider));
        assert_eq!(registry.len(), 1);
    }
}
