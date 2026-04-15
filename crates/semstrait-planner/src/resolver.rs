//! Expression resolver: Expr → Expr with column name resolution.
//!
//! Resolves entity refs and column names from semantic to physical using
//! dataset column mappings. Also expands Guard sugar to Case.
//!
//! Two implementations:
//! - [`MappingResolver`]: resolves via `HashMap<String, ColumnMappingValue>` (legacy path)
//! - [`PhysicalResolver`]: resolves via `IndexMap<String, String>` (v2 pre-resolved path)

use semstrait_core::expr::WhenClause;
use semstrait_core::Expr;

use crate::error::PlannerError;
use indexmap::IndexMap;

#[cfg(test)]
use semstrait_manifest::{ColumnMappingValue, LiteralValue};
#[cfg(test)]
use std::collections::HashMap;

/// Trait for resolving semantic column names to physical expressions.
pub trait ExprResolver {
    /// Resolve a single column/entity name to its physical expression.
    fn resolve_column(&self, name: &str) -> Expr;

    /// Resolve an entire expression tree, rewriting column names and
    /// expanding Guard → Case.
    fn resolve_expr(&self, expr: &Expr) -> Result<Expr, PlannerError> {
        expr.transform(&|e| match e {
            Expr::Column(col) => Ok(Some(self.resolve_column(&col.name))),
            Expr::EntityRef(entity) => Ok(Some(self.resolve_column(&entity.name))),
            Expr::Guard(g) => Ok(Some(Expr::case(
                vec![WhenClause::new((*g.condition).clone(), (*g.expr).clone())],
                Some(Expr::null()),
            ))),
            _ => Ok(None),
        })
    }
}

/// Resolves names via `HashMap<String, ColumnMappingValue>`.
///
/// Used in tests for the legacy column mapping path.
#[cfg(test)]
pub struct MappingResolver<'a> {
    pub mapping: &'a HashMap<String, ColumnMappingValue>,
}

#[cfg(test)]
impl<'a> MappingResolver<'a> {
    pub fn new(mapping: &'a HashMap<String, ColumnMappingValue>) -> Self {
        Self { mapping }
    }
}

#[cfg(test)]
impl ExprResolver for MappingResolver<'_> {
    fn resolve_column(&self, name: &str) -> Expr {
        match self.mapping.get(name) {
            Some(ColumnMappingValue::Simple(s)) => Expr::column(s.clone()),
            Some(ColumnMappingValue::WithGrain { column, .. }) => Expr::column(column.clone()),
            Some(ColumnMappingValue::Literal(lit)) => match lit {
                LiteralValue::String(s) => Expr::string(s.clone()),
            },
            Some(ColumnMappingValue::Anchored(_)) => Expr::column(name),
            None => Expr::column(name),
        }
    }
}

/// Resolves names via `IndexMap<String, String>` (pre-resolved physical mapping).
///
/// Used by all kind planners (grainset, unionset, joinset, dataset) in the
/// v2 `ResolvedColumnMapping.physical` path.
pub struct PhysicalResolver<'a> {
    pub physical: &'a IndexMap<String, String>,
}

impl<'a> PhysicalResolver<'a> {
    pub fn new(physical: &'a IndexMap<String, String>) -> Self {
        Self { physical }
    }
}

impl ExprResolver for PhysicalResolver<'_> {
    fn resolve_column(&self, name: &str) -> Expr {
        match self.physical.get(name) {
            Some(phys) => Expr::column(phys.clone()),
            None => Expr::column(name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_mapping() -> HashMap<String, ColumnMappingValue> {
        let mut m = HashMap::new();
        m.insert(
            "revenue".to_string(),
            ColumnMappingValue::Simple("amount".to_string()),
        );
        m.insert(
            "cost".to_string(),
            ColumnMappingValue::Simple("cost_usd".to_string()),
        );
        m.insert(
            "region".to_string(),
            ColumnMappingValue::Simple("region_name".to_string()),
        );
        m.insert(
            "order_count".to_string(),
            ColumnMappingValue::Simple("order_id".to_string()),
        );
        m
    }

    fn test_physical() -> IndexMap<String, String> {
        let mut m = IndexMap::new();
        m.insert("revenue".to_string(), "amount".to_string());
        m.insert("cost".to_string(), "cost_usd".to_string());
        m.insert("region".to_string(), "region_name".to_string());
        m.insert("order_count".to_string(), "order_id".to_string());
        m
    }

    // ── MappingResolver tests ──────────────────────────────────────────

    #[test]
    fn test_mapping_resolver_column() {
        let mapping = test_mapping();
        let resolver = MappingResolver::new(&mapping);
        let expr = Expr::column("revenue");
        let resolved = resolver.resolve_expr(&expr).unwrap();
        assert_eq!(resolved, Expr::column("amount"));
    }

    #[test]
    fn test_mapping_resolver_passthrough() {
        let mapping = test_mapping();
        let resolver = MappingResolver::new(&mapping);
        let expr = Expr::column("unknown_col");
        let resolved = resolver.resolve_expr(&expr).unwrap();
        assert_eq!(resolved, Expr::column("unknown_col"));
    }

    #[test]
    fn test_mapping_resolver_entity_ref() {
        let mapping = test_mapping();
        let resolver = MappingResolver::new(&mapping);
        let expr = Expr::entity_ref("region");
        let resolved = resolver.resolve_expr(&expr).unwrap();
        assert_eq!(resolved, Expr::column("region_name"));
    }

    #[test]
    fn test_mapping_resolver_literals() {
        let mapping = test_mapping();
        let resolver = MappingResolver::new(&mapping);

        assert_eq!(resolver.resolve_expr(&Expr::int(42)).unwrap(), Expr::int(42));
        assert_eq!(resolver.resolve_expr(&Expr::float(2.72)).unwrap(), Expr::float(2.72));
        assert_eq!(resolver.resolve_expr(&Expr::string("hello")).unwrap(), Expr::string("hello"));
        assert_eq!(resolver.resolve_expr(&Expr::boolean(true)).unwrap(), Expr::boolean(true));
        assert_eq!(resolver.resolve_expr(&Expr::null()).unwrap(), Expr::null());
    }

    #[test]
    fn test_mapping_resolver_binary_arithmetic() {
        let mapping = test_mapping();
        let resolver = MappingResolver::new(&mapping);
        let expr = Expr::add(Expr::column("revenue"), Expr::int(10));
        let resolved = resolver.resolve_expr(&expr).unwrap();
        assert_eq!(resolved, Expr::add(Expr::column("amount"), Expr::int(10)));
    }

    #[test]
    fn test_mapping_resolver_case_expr() {
        let mapping = test_mapping();
        let resolver = MappingResolver::new(&mapping);
        let expr = Expr::case(
            vec![WhenClause::new(
                Expr::eq(Expr::column("region"), Expr::string("US")),
                Expr::int(1),
            )],
            Some(Expr::int(0)),
        );
        let resolved = resolver.resolve_expr(&expr).unwrap();
        match resolved {
            Expr::Case(case) => {
                assert_eq!(case.when_then.len(), 1);
                assert!(case.else_expr.is_some());
            }
            other => panic!("Expected Case, got {:?}", other),
        }
    }

    #[test]
    fn test_mapping_resolver_guard_becomes_case() {
        let mapping = test_mapping();
        let resolver = MappingResolver::new(&mapping);
        let expr = Expr::guard(
            Expr::eq(Expr::column("region"), Expr::string("US")),
            Expr::column("revenue"),
        );
        let resolved = resolver.resolve_expr(&expr).unwrap();
        match resolved {
            Expr::Case(case) => {
                assert_eq!(case.when_then.len(), 1);
                assert_eq!(case.else_expr, Some(Box::new(Expr::null())));
            }
            other => panic!("Expected Case from Guard, got {:?}", other),
        }
    }

    // ── PhysicalResolver tests ─────────────────────────────────────────

    #[test]
    fn test_physical_resolver_column() {
        let physical = test_physical();
        let resolver = PhysicalResolver::new(&physical);
        let expr = Expr::column("revenue");
        let resolved = resolver.resolve_expr(&expr).unwrap();
        assert_eq!(resolved, Expr::column("amount"));
    }

    #[test]
    fn test_physical_resolver_passthrough() {
        let physical = test_physical();
        let resolver = PhysicalResolver::new(&physical);
        let expr = Expr::column("unknown_col");
        let resolved = resolver.resolve_expr(&expr).unwrap();
        assert_eq!(resolved, Expr::column("unknown_col"));
    }

    #[test]
    fn test_physical_resolver_entity_ref() {
        let physical = test_physical();
        let resolver = PhysicalResolver::new(&physical);
        let expr = Expr::entity_ref("region");
        let resolved = resolver.resolve_expr(&expr).unwrap();
        assert_eq!(resolved, Expr::column("region_name"));
    }

    #[test]
    fn test_physical_resolver_guard() {
        let physical = test_physical();
        let resolver = PhysicalResolver::new(&physical);
        let expr = Expr::guard(
            Expr::eq(Expr::column("region"), Expr::string("US")),
            Expr::column("revenue"),
        );
        let resolved = resolver.resolve_expr(&expr).unwrap();
        match resolved {
            Expr::Case(case) => {
                assert_eq!(case.when_then.len(), 1);
                assert_eq!(case.else_expr, Some(Box::new(Expr::null())));
            }
            other => panic!("Expected Case from Guard, got {:?}", other),
        }
    }

    #[test]
    fn test_both_resolvers_agree() {
        // For entries that exist in both mappings, results should match.
        let mapping = test_mapping();
        let physical = test_physical();
        let mr = MappingResolver::new(&mapping);
        let pr = PhysicalResolver::new(&physical);

        let expr = Expr::add(Expr::column("revenue"), Expr::column("cost"));
        let from_mapping = mr.resolve_expr(&expr).unwrap();
        let from_physical = pr.resolve_expr(&expr).unwrap();
        assert_eq!(from_mapping, from_physical);
    }
}
