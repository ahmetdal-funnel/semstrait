//! Optimizer — internal plan quality pass (empty in v1).
//!
//! The Optimizer applies a chain of OptimizerPass implementations to a LogicalPlan.
//! In v1, zero passes are registered — the optimizer is an identity function.
//! Passes are opt-in at SemanticPlanner construction via the builder.

use crate::error::PlannerError;
use semstrait_ir::LogicalPlan;

/// A single optimization pass that transforms a LogicalPlan.
pub trait OptimizerPass: Send + Sync {
    /// Human-readable name for logging and error reporting.
    fn name(&self) -> &str;

    /// Apply this pass to the plan, returning a potentially modified plan.
    fn apply(&self, plan: LogicalPlan) -> Result<LogicalPlan, PlannerError>;

    /// Returns true if this pass is applicable to the given plan.
    /// Default: always applicable.
    fn is_applicable(&self, _plan: &LogicalPlan) -> bool {
        true
    }
}

/// The optimizer applies a chain of passes to a LogicalPlan.
pub struct Optimizer {
    passes: Vec<Box<dyn OptimizerPass>>,
}

impl Optimizer {
    /// Create an empty optimizer (identity function).
    pub fn empty() -> Self {
        Self { passes: vec![] }
    }

    /// Create an optimizer with the given passes.
    pub fn new(passes: Vec<Box<dyn OptimizerPass>>) -> Self {
        Self { passes }
    }

    /// Apply all passes in order, short-circuiting on error.
    pub fn apply(&self, plan: LogicalPlan) -> Result<LogicalPlan, PlannerError> {
        self.passes.iter().try_fold(plan, |plan, pass| {
            if pass.is_applicable(&plan) {
                tracing::debug!("applying optimizer pass: {}", pass.name());
                pass.apply(plan)
            } else {
                tracing::debug!("skipping optimizer pass: {} (not applicable)", pass.name());
                Ok(plan)
            }
        })
    }

    /// Returns the number of registered passes.
    #[allow(dead_code)] // Used in tests; public API for future introspection
    pub(crate) fn pass_count(&self) -> usize {
        self.passes.len()
    }
}

impl Default for Optimizer {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use semstrait_core::DataType;
    use semstrait_ir::{Field, NodeMeta, PlanNode, ScanNode, Schema};

    /// A test pass that simply returns the plan unchanged.
    struct IdentityPass;

    impl OptimizerPass for IdentityPass {
        fn name(&self) -> &str {
            "identity"
        }

        fn apply(&self, plan: LogicalPlan) -> Result<LogicalPlan, PlannerError> {
            Ok(plan)
        }
    }

    /// A test pass that always fails.
    struct FailingPass;

    impl OptimizerPass for FailingPass {
        fn name(&self) -> &str {
            "failing"
        }

        fn apply(&self, _plan: LogicalPlan) -> Result<LogicalPlan, PlannerError> {
            Err(PlannerError::OptimizerError {
                pass: "failing".to_string(),
                reason: "intentional failure".to_string(),
            })
        }
    }

    /// A test pass that only applies to plans with more than 0 output names.
    struct ConditionalPass;

    impl OptimizerPass for ConditionalPass {
        fn name(&self) -> &str {
            "conditional"
        }

        fn apply(&self, plan: LogicalPlan) -> Result<LogicalPlan, PlannerError> {
            Ok(plan)
        }

        fn is_applicable(&self, plan: &LogicalPlan) -> bool {
            !plan.output_names.is_empty()
        }
    }

    fn make_test_plan() -> LogicalPlan {
        let schema = Schema::new(vec![Field::new("id", DataType::Integer)]);
        let scan = PlanNode::Scan(ScanNode {
            meta: NodeMeta::new(schema),
            table_name: "test_table".to_string(),
            location: None,
            format: None,
            projection: vec!["id".to_string()],
        });
        LogicalPlan::new(scan, vec!["id".to_string()])
    }

    #[test]
    fn test_empty_optimizer_is_identity() {
        let optimizer = Optimizer::empty();
        assert_eq!(optimizer.pass_count(), 0);

        let plan = make_test_plan();
        let result = optimizer.apply(plan);
        assert!(result.is_ok());
        let plan = result.unwrap();
        assert_eq!(plan.output_names, vec!["id".to_string()]);
    }

    #[test]
    fn test_identity_pass() {
        let optimizer = Optimizer::new(vec![Box::new(IdentityPass)]);
        let plan = make_test_plan();
        let result = optimizer.apply(plan);
        assert!(result.is_ok());
    }

    #[test]
    fn test_failing_pass() {
        let optimizer = Optimizer::new(vec![Box::new(FailingPass)]);
        let plan = make_test_plan();
        let result = optimizer.apply(plan);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PlannerError::OptimizerError { .. }
        ));
    }

    #[test]
    fn test_conditional_pass_skipped() {
        let optimizer = Optimizer::new(vec![Box::new(ConditionalPass)]);
        // Plan with empty output names — conditional pass should be skipped.
        let schema = Schema::new(vec![Field::new("id", DataType::Integer)]);
        let scan = PlanNode::Scan(ScanNode {
            meta: NodeMeta::new(schema),
            table_name: "test".to_string(),
            location: None,
            format: None,
            projection: vec!["id".to_string()],
        });
        let plan = LogicalPlan::new(scan, vec![]);
        let result = optimizer.apply(plan);
        assert!(result.is_ok());
    }
}
