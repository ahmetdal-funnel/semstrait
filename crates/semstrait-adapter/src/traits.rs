//! EngineAdapter trait — the core abstraction for engine-specific plan adaptation.

use semstrait_ir::{LogicalPlan, PlanArtifact, PlanBuilder};
use crate::sql::{AnsiDialect, AnsiSqlEmitter, SqlEmitter};

use crate::AdaptError;

/// Produces an engine-appropriate artifact from a LogicalPlan.
///
/// Each engine adapter converts a logical plan into an engine-specific artifact
/// (SQL string or Substrait plan). The adapter knows its target engine and
/// produces the appropriate output format.
///
/// Adapters may also provide an engine-specific `PlanBuilder` that overrides
/// how plan nodes are constructed (e.g., custom scan nodes for catalog-aware engines).
pub trait EngineAdapter: Send + Sync {
    /// Human-readable engine name (e.g., "datafusion", "duckdb", "spark").
    fn name(&self) -> &str;

    /// Return an engine-specific plan builder, or `None` for default behavior.
    ///
    /// When provided, the planner uses this builder to construct plan nodes,
    /// allowing engines to override specific node construction (e.g., scan nodes
    /// with engine-specific metadata, custom function anchors).
    fn plan_builder(&self) -> Option<Box<dyn PlanBuilder>> {
        None
    }

    /// Convert a LogicalPlan into an engine-ready artifact.
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
    use semstrait_ir::{
        AggNode, AggregateMeasure, Aggregation, Expr, Field, LogicalPlan, NodeMeta, PlanArtifact,
        PlanNode, ScanNode, Schema,
    };

    use semstrait_core::DataType;

    /// A minimal adapter for testing the trait without feature flags.
    struct TestAdapter;

    impl EngineAdapter for TestAdapter {
        fn name(&self) -> &str {
            "test"
        }

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
            Field::new("region", DataType::String),
            Field::new("revenue", DataType::Number),
        ]);

        let scan_schema = Schema::new(vec![
            Field::new("region", DataType::String),
            Field::new("amount", DataType::Number),
        ]);

        let scan = PlanNode::Scan(ScanNode {
            meta: NodeMeta::new(scan_schema),
            table_name: "orders_daily".to_string(),
            location: None,
            format: None,
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
                data_type: semstrait_core::DataType::Number,
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
