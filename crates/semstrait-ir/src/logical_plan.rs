//! LogicalPlan wrapper

use crate::plan_node::PlanNode;

/// A complete logical query plan
#[derive(Debug, Clone)]
pub struct LogicalPlan {
    /// Root node of the plan tree
    pub root: PlanNode,
    /// Semantic output column names (e.g., dimension/measure names)
    pub output_names: Vec<String>,
}

impl LogicalPlan {
    pub fn new(root: PlanNode, output_names: Vec<String>) -> Self {
        Self { root, output_names }
    }
}
