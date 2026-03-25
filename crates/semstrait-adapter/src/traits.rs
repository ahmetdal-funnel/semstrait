//! EngineAdapter trait — the core abstraction for engine-specific plan adaptation.

use semstrait_core::EngineProfile;
use semstrait_ir::{LogicalPlan, PlanArtifact};
use semstrait_sql::{AnsiDialect, AnsiSqlEmitter, SqlEmitter};

use crate::AdaptError;

/// Produces an engine-appropriate artifact from a LogicalPlan.
///
/// Each engine adapter:
/// 1. Implements `EngineProfile` (capability flags)
/// 2. Implements `EngineAdapter` (plan -> artifact conversion)
///
/// The adapter inspects its own profile to decide whether to emit SQL or Substrait.
pub trait EngineAdapter: EngineProfile {
    /// Convert a LogicalPlan into an engine-ready artifact.
    ///
    /// - If `supports_substrait()` is true, produces `PlanArtifact::Substrait`.
    /// - Otherwise, produces `PlanArtifact::Sql` using the appropriate dialect.
    fn adapt(&self, plan: &LogicalPlan) -> Result<PlanArtifact, AdaptError>;

    /// Generate SQL from a LogicalPlan for debugging purposes.
    ///
    /// Always available regardless of the primary artifact type.
    /// When the primary artifact is already SQL, returns the same output.
    /// When the primary artifact is Substrait, generates ANSI SQL as a
    /// human-readable representation for debugging and troubleshooting.
    fn debug_sql(&self, plan: &LogicalPlan) -> Result<String, AdaptError> {
        let emitter = AnsiSqlEmitter::new(AnsiDialect);
        emitter
            .emit(plan)
            .map_err(|e| AdaptError::SqlEmission(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use semstrait_core::EngineProfile;
    use semstrait_ir::{
        AggNode, AggregateMeasure, Aggregation, Expr, Field, LogicalPlan, NodeMeta, PlanArtifact,
        PlanNode, ScanNode, Schema,
    };

    use semstrait_core::DataType;

    /// A minimal adapter for testing the trait without feature flags.
    struct TestAdapter;

    impl EngineProfile for TestAdapter {
        fn name(&self) -> &str {
            "test"
        }
        fn supports_substrait(&self) -> bool {
            false
        }
        fn supports_window_functions(&self) -> bool {
            true
        }
        fn supports_full_outer_join(&self) -> bool {
            true
        }
        fn supports_cte(&self) -> bool {
            true
        }
        fn supports_subquery(&self) -> bool {
            true
        }
        fn supports_inline_views(&self) -> bool {
            true
        }
        fn supports_fetch_rel(&self) -> bool {
            true
        }
        fn max_join_depth(&self) -> Option<usize> {
            Some(10)
        }
    }

    impl EngineAdapter for TestAdapter {
        fn adapt(&self, plan: &LogicalPlan) -> Result<PlanArtifact, AdaptError> {
            let emitter = AnsiSqlEmitter::new(AnsiDialect);
            let sql = emitter
                .emit(plan)
                .map_err(|e| AdaptError::SqlEmission(e.to_string()))?;
            Ok(PlanArtifact::Sql(sql))
        }
    }

    fn make_test_plan() -> LogicalPlan {
        let schema = Schema::new(vec![
            Field::new("region", DataType::Utf8),
            Field::new("revenue", DataType::Float64),
        ]);

        let scan_schema = Schema::new(vec![
            Field::new("region", DataType::Utf8),
            Field::new("amount", DataType::Float64),
        ]);

        let scan = PlanNode::Scan(ScanNode {
            meta: NodeMeta::new(scan_schema),
            table_name: "orders_daily".to_string(),
            projection: vec!["region".to_string(), "amount".to_string()],
        });

        let agg = PlanNode::Aggregate(AggNode {
            meta: NodeMeta::new(schema),
            input: Box::new(scan),
            group_by: vec![Expr::column("region")],
            aggregates: vec![AggregateMeasure {
                function: Aggregation::Sum,
                expr: Expr::column("amount"),
                distinct: false,
            }],
        });

        LogicalPlan::new(agg, vec!["region".to_string(), "revenue".to_string()])
    }

    #[test]
    fn test_adapter_trait_object() {
        let adapter = TestAdapter;
        let dyn_adapter: &dyn EngineAdapter = &adapter;

        assert_eq!(dyn_adapter.name(), "test");

        let plan = make_test_plan();
        let artifact = dyn_adapter.adapt(&plan).unwrap();
        assert!(artifact.is_sql());
    }

    #[test]
    fn test_default_debug_sql() {
        let adapter = TestAdapter;
        let plan = make_test_plan();

        let sql = adapter.debug_sql(&plan).unwrap();
        assert!(sql.contains("SELECT"), "debug_sql should produce SQL: {sql}");
        assert!(
            sql.contains("region"),
            "debug_sql should reference 'region': {sql}"
        );
    }

    #[test]
    fn test_adapter_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<TestAdapter>();
    }
}
