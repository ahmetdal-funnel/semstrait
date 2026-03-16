//! Metric chaining depth validation.
//!
//! Metrics can reference other metrics in their `expr` DSL. This module
//! validates that the chain depth does not exceed the maximum allowed (3).
//! Deeper chains make debugging and performance analysis difficult.

use std::collections::{HashMap, HashSet};

use crate::diagnostics::{codes, Diagnostic};

/// Maximum allowed metric chaining depth (metric → metric → measure).
const MAX_CHAIN_DEPTH: usize = 3;

/// Validate that no metric chain exceeds the maximum depth.
///
/// `dependencies` maps each metric name to the names it references
/// (other metrics or measures). Measures have no entries in the map.
///
/// Returns `Ok(())` if all chains are within limits, or a list of
/// diagnostics for each violation.
pub fn validate_metric_depth(
    dependencies: &HashMap<String, Vec<String>>,
) -> Result<(), Vec<Diagnostic>> {
    let mut errors = Vec::new();

    for name in dependencies.keys() {
        let mut visited = HashSet::new();
        let depth = chain_depth(name, dependencies, &mut visited);
        if depth > MAX_CHAIN_DEPTH {
            errors.push(
                Diagnostic::error(
                    codes::METRC_E001,
                    format!(
                        "metric '{}' has chain depth {} (max {})",
                        name, depth, MAX_CHAIN_DEPTH
                    ),
                )
                .with_entity(format!("metrics.{}", name), name),
            );
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Recursively compute the chain depth for a metric.
/// Returns 1 for a leaf (measure), 2 for metric→measure, etc.
fn chain_depth(
    name: &str,
    dependencies: &HashMap<String, Vec<String>>,
    visited: &mut HashSet<String>,
) -> usize {
    // Cycle detection
    if !visited.insert(name.to_string()) {
        // Circular reference — return a large number to trigger the limit
        return MAX_CHAIN_DEPTH + 1;
    }

    match dependencies.get(name) {
        None => 1, // leaf (measure or unknown) — depth 1
        Some(deps) if deps.is_empty() => 1,
        Some(deps) => {
            let max_child = deps
                .iter()
                .map(|d| chain_depth(d, dependencies, &mut visited.clone()))
                .max()
                .unwrap_or(0);
            1 + max_child
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_metric_measure_ok() {
        // metric_a → measure_x (depth 2, ok)
        let mut deps = HashMap::new();
        deps.insert("metric_a".into(), vec!["measure_x".into()]);
        assert!(validate_metric_depth(&deps).is_ok());
    }

    #[test]
    fn test_two_level_chain_ok() {
        // metric_a → metric_b → measure_x (depth 3, ok)
        let mut deps = HashMap::new();
        deps.insert("metric_a".into(), vec!["metric_b".into()]);
        deps.insert("metric_b".into(), vec!["measure_x".into()]);
        assert!(validate_metric_depth(&deps).is_ok());
    }

    #[test]
    fn test_three_level_chain_exceeds() {
        // metric_a → metric_b → metric_c → measure_x (depth 4, fail)
        let mut deps = HashMap::new();
        deps.insert("metric_a".into(), vec!["metric_b".into()]);
        deps.insert("metric_b".into(), vec!["metric_c".into()]);
        deps.insert("metric_c".into(), vec!["measure_x".into()]);
        let errors = validate_metric_depth(&deps).unwrap_err();
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.code == codes::METRC_E001));
    }

    #[test]
    fn test_circular_reference_detected() {
        let mut deps = HashMap::new();
        deps.insert("a".into(), vec!["b".into()]);
        deps.insert("b".into(), vec!["a".into()]);
        let errors = validate_metric_depth(&deps).unwrap_err();
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_diamond_dependency_ok() {
        // a → b, a → c, b → m, c → m (depth 2 via b or c)
        let mut deps = HashMap::new();
        deps.insert("a".into(), vec!["b".into(), "c".into()]);
        deps.insert("b".into(), vec!["m".into()]);
        deps.insert("c".into(), vec!["m".into()]);
        assert!(validate_metric_depth(&deps).is_ok());
    }
}
