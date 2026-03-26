//! Spark adapter — produces SQL with ANSI dialect (Spark-compatible).

use semstrait_core::EngineProfile;
use semstrait_ir::{LogicalPlan, PlanArtifact};
use semstrait_sql::{AnsiDialect, AnsiSqlEmitter, SqlEmitter};

use crate::{AdaptError, EngineAdapter};

/// Spark adapter — produces SQL with ANSI dialect.
///
/// Spark SQL is largely ANSI-compatible. This adapter uses the ANSI dialect
/// for SQL generation. A dedicated SparkDialect may be introduced later
/// for engine-specific idioms.
pub struct SparkAdapter;

impl EngineProfile for SparkAdapter {
    fn name(&self) -> &str {
        "spark"
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

impl EngineAdapter for SparkAdapter {
    fn adapt(&self, plan: &LogicalPlan) -> Result<PlanArtifact, AdaptError> {
        let emitter = AnsiSqlEmitter::new(AnsiDialect);
        let sql = emitter
            .emit(plan)
            .map_err(|e| AdaptError::SqlEmission(e.to_string()))?;
        Ok(PlanArtifact::Sql(sql))
    }
    // debug_sql() uses default ANSI implementation (same as adapt output)
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
            }],
        });

        LogicalPlan::new(agg, vec!["region".to_string(), "revenue".to_string()])
    }

    #[test]
    fn test_spark_adapter_produces_sql() {
        let adapter = SparkAdapter;
        let plan = make_test_plan();
        let artifact = adapter.adapt(&plan).unwrap();
        assert!(artifact.is_sql(), "Spark adapter should produce SQL");
        let sql = artifact.as_sql().unwrap();
        assert!(sql.contains("SELECT"), "SQL should contain SELECT: {sql}");
    }

    #[test]
    fn test_spark_profile() {
        let adapter = SparkAdapter;
        assert_eq!(adapter.name(), "spark");
        assert!(!adapter.supports_substrait());
        assert!(adapter.supports_window_functions());
        assert!(adapter.supports_full_outer_join());
        assert!(adapter.supports_cte());
        assert!(adapter.supports_subquery());
        assert!(adapter.supports_inline_views());
        assert!(adapter.supports_fetch_rel());
        assert_eq!(adapter.max_join_depth(), Some(10));
    }

    #[test]
    fn test_spark_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SparkAdapter>();
    }
}
