//! LogicalPlan wrapper

use super::node::PlanNode;

/// A complete logical query plan
#[derive(Debug, Clone)]
pub struct LogicalPlan {
    /// Root node of the plan tree
    pub root: PlanNode,
    /// Semantic output column names (e.g., dimension/measure names)
    pub output_names: Vec<String>,
    /// Non-fatal warnings collected during planning
    pub warnings: Vec<PlannerWarning>,
}

impl LogicalPlan {
    pub fn new(root: PlanNode, output_names: Vec<String>) -> Self {
        Self {
            root,
            output_names,
            warnings: Vec::new(),
        }
    }

    /// Create a plan with warnings.
    pub fn with_warnings(
        root: PlanNode,
        output_names: Vec<String>,
        warnings: Vec<PlannerWarning>,
    ) -> Self {
        Self {
            root,
            output_names,
            warnings,
        }
    }
}

/// Non-fatal warnings produced during query planning.
///
/// Warnings do not prevent plan execution but signal potential issues
/// (e.g., schema drift).
#[derive(Debug, Clone, PartialEq)]
pub enum PlannerWarning {
    /// PLAN_W003: Physical schema has changed since the manifest was compiled.
    SchemaDrift {
        dataset: String,
        details: String,
    },
}
