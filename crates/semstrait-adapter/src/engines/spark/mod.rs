//! Spark adapter — produces SQL with Spark dialect.
//!
//! V1: Spark adapter is not yet supported. The SQL dialect and emitter
//! infrastructure exists but has not been validated against a live Spark
//! engine. Use DataFusion as the primary compute engine in V1.

use semstrait_ir::{LogicalPlan, PlanArtifact};

use crate::{AdaptError, EngineAdapter};

/// Spark adapter — produces SQL with Spark dialect.
///
/// **V1 status: unsupported.** The Spark SQL dialect exists but the
/// adapter has not been validated against a live Spark engine.
/// Use `DataFusionAdapter` as the primary compute engine in V1.
pub struct SparkAdapter;

impl EngineAdapter for SparkAdapter {
    fn name(&self) -> &str {
        "spark"
    }

    fn adapt(&self, _plan: &LogicalPlan) -> Result<PlanArtifact, AdaptError> {
        Err(AdaptError::UnsupportedFeature(
            "Spark adapter is not supported in V1. Use DataFusion.".to_string(),
        ))
    }

    // debug_sql() inherits ANSI default from EngineAdapter trait (constraint E7).
}

#[cfg(test)]
mod tests {
    use super::*;
    use semstrait_core::DataType;
    use semstrait_ir::{
        AggNode, AggregateMeasure, Aggregation, Expr, Field, LogicalPlan, NodeMeta, PlanNode,
        ScanNode, Schema,
    };

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
    fn test_spark_adapt_unsupported() {
        let adapter = SparkAdapter;
        let plan = make_test_plan();
        let result = adapter.adapt(&plan);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not supported in V1"),
            "Expected V1 unsupported error, got: {err}"
        );
    }

    #[test]
    fn test_spark_debug_sql_ansi_fallback() {
        // E7: debug_sql() always available — inherits ANSI default from trait.
        let adapter = SparkAdapter;
        let plan = make_test_plan();
        let sql = adapter.debug_sql(&plan).expect("debug_sql should succeed (E7)");
        assert!(sql.contains("SELECT"), "debug_sql should produce SQL: {sql}");
        assert!(sql.contains("region"), "debug_sql should reference 'region': {sql}");
    }

    #[test]
    fn test_spark_name() {
        let adapter = SparkAdapter;
        assert_eq!(adapter.name(), "spark");
    }

    #[test]
    fn test_spark_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SparkAdapter>();
    }
}
