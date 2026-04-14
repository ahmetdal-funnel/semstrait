//! DuckDB adapter — produces SQL with DuckDB dialect.
//!
//! V1: DuckDB adapter is not yet supported. The SQL dialect and emitter
//! infrastructure exists but has not been validated against a live DuckDB
//! engine. Use DataFusion as the primary compute engine in V1.

use semstrait_ir::{LogicalPlan, PlanArtifact};

use crate::{AdaptError, EngineAdapter};

/// DuckDB adapter — produces SQL with DuckDB dialect.
///
/// **V1 status: unsupported.** The DuckDB SQL dialect exists but the
/// adapter has not been validated against a live DuckDB engine.
/// Use `DataFusionAdapter` as the primary compute engine in V1.
pub struct DuckDbAdapter;

impl EngineAdapter for DuckDbAdapter {
    fn name(&self) -> &str {
        "duckdb"
    }

    fn adapt(&self, _plan: &LogicalPlan) -> Result<PlanArtifact, AdaptError> {
        Err(AdaptError::UnsupportedFeature(
            "DuckDB adapter is not supported in V1. Use DataFusion.".to_string(),
        ))
    }

    fn debug_sql(&self, _plan: &LogicalPlan) -> Result<String, AdaptError> {
        Err(AdaptError::UnsupportedFeature(
            "DuckDB adapter is not supported in V1. Use DataFusion.".to_string(),
        ))
    }
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
    fn test_duckdb_adapt_unsupported() {
        let adapter = DuckDbAdapter;
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
    fn test_duckdb_debug_sql_unsupported() {
        let adapter = DuckDbAdapter;
        let plan = make_test_plan();
        let result = adapter.debug_sql(&plan);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not supported in V1"),
            "Expected V1 unsupported error, got: {err}"
        );
    }

    #[test]
    fn test_duckdb_name() {
        let adapter = DuckDbAdapter;
        assert_eq!(adapter.name(), "duckdb");
    }

    #[test]
    fn test_duckdb_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DuckDbAdapter>();
    }
}
