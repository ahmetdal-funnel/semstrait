//! Spark adapter — produces SQL with Spark dialect.

use semstrait_ir::{LogicalPlan, PlanArtifact};
use crate::sql::{AnsiSqlEmitter, SparkDialect, SqlEmitter};

use crate::{AdaptError, EngineAdapter};

/// Spark adapter — produces SQL with Spark dialect.
///
/// Spark SQL uses LIMIT syntax and `current_timestamp()` function form.
/// This adapter uses the SparkDialect for SQL generation.
pub struct SparkAdapter;

impl EngineAdapter for SparkAdapter {
    fn name(&self) -> &str {
        "spark"
    }

    fn adapt(&self, plan: &LogicalPlan) -> Result<PlanArtifact, AdaptError> {
        let emitter = AnsiSqlEmitter::new(SparkDialect);
        let sql = emitter
            .emit(plan)
            .map_err(|e| AdaptError::SqlEmission(e.to_string()))?;
        Ok(PlanArtifact::Sql(sql))
    }

    fn debug_sql(&self, plan: &LogicalPlan) -> Result<String, AdaptError> {
        let emitter = AnsiSqlEmitter::new(SparkDialect);
        emitter
            .emit(plan)
            .map_err(|e| AdaptError::SqlEmission(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use semstrait_core::DataType;
    use semstrait_ir::{
        AggNode, AggregateMeasure, Aggregation, Expr, FetchNode, Field, LogicalPlan, NodeMeta,
        PlanNode, ScanNode, Schema,
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
    fn test_spark_adapter_produces_sql() {
        let adapter = SparkAdapter;
        let plan = make_test_plan();
        let artifact = adapter.adapt(&plan).unwrap();
        assert!(artifact.is_sql(), "Spark adapter should produce SQL");
        let sql = artifact.as_sql().unwrap();
        assert!(sql.contains("SELECT"), "SQL should contain SELECT: {sql}");
    }

    #[test]
    fn test_spark_sql_uses_limit() {
        let schema = Schema::new(vec![
            Field::new("id", DataType::Integer),
            Field::new("amount", DataType::Number),
        ]);

        let scan = PlanNode::Scan(ScanNode {
            meta: NodeMeta::new(schema.clone()),
            table_name: "orders".to_string(),
            location: None,
            format: None,
            projection: vec!["id".to_string(), "amount".to_string()],
        });

        let fetch = PlanNode::Fetch(FetchNode {
            meta: NodeMeta::new(schema),
            input: Box::new(scan),
            count: Some(10),
            offset: 0,
        });

        let plan = LogicalPlan::new(fetch, vec!["id".to_string(), "amount".to_string()]);

        let adapter = SparkAdapter;
        let artifact = adapter.adapt(&plan).unwrap();
        let sql = artifact.as_sql().unwrap();
        assert!(
            sql.contains("LIMIT"),
            "Spark SQL should use LIMIT, got: {sql}"
        );
        assert!(
            !sql.contains("FETCH FIRST"),
            "Spark SQL should NOT use FETCH FIRST, got: {sql}"
        );
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
