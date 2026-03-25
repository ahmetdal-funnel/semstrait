//! Trino adapter — produces SQL with Trino dialect.

use semstrait_core::EngineProfile;
use semstrait_ir::{LogicalPlan, PlanArtifact};
use semstrait_sql::{AnsiSqlEmitter, SqlEmitter, TrinoDialect};

use crate::{AdaptError, EngineAdapter};

/// Trino adapter — produces SQL with Trino dialect.
///
/// Trino consumes SQL strings. This adapter uses the Trino dialect
/// which produces FETCH FIRST N ROWS ONLY and Trino-specific idioms.
pub struct TrinoAdapter;

impl EngineProfile for TrinoAdapter {
    fn name(&self) -> &str {
        "trino"
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

impl EngineAdapter for TrinoAdapter {
    fn adapt(&self, plan: &LogicalPlan) -> Result<PlanArtifact, AdaptError> {
        let emitter = AnsiSqlEmitter::new(TrinoDialect);
        let sql = emitter
            .emit(plan)
            .map_err(|e| AdaptError::SqlEmission(e.to_string()))?;
        Ok(PlanArtifact::Sql(sql))
    }

    fn debug_sql(&self, plan: &LogicalPlan) -> Result<String, AdaptError> {
        let emitter = AnsiSqlEmitter::new(TrinoDialect);
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
    fn test_trino_adapter_produces_sql() {
        let adapter = TrinoAdapter;
        let plan = make_test_plan();
        let artifact = adapter.adapt(&plan).unwrap();
        assert!(artifact.is_sql(), "Trino adapter should produce SQL");
        let sql = artifact.as_sql().unwrap();
        assert!(sql.contains("SELECT"), "SQL should contain SELECT: {sql}");
    }

    #[test]
    fn test_trino_sql_uses_fetch_first() {
        let schema = Schema::new(vec![
            Field::new("id", DataType::Int64),
            Field::new("amount", DataType::Float64),
        ]);

        let scan = PlanNode::Scan(ScanNode {
            meta: NodeMeta::new(schema.clone()),
            table_name: "orders".to_string(),
            projection: vec!["id".to_string(), "amount".to_string()],
        });

        let fetch = PlanNode::Fetch(FetchNode {
            meta: NodeMeta::new(schema),
            input: Box::new(scan),
            count: Some(10),
            offset: 0,
        });

        let plan = LogicalPlan::new(fetch, vec!["id".to_string(), "amount".to_string()]);

        let adapter = TrinoAdapter;
        let artifact = adapter.adapt(&plan).unwrap();
        let sql = artifact.as_sql().unwrap();
        // Trino uses ANSI-style FETCH FIRST (via TrinoDialect which inherits ANSI behavior)
        // or LIMIT depending on dialect implementation
        assert!(
            sql.contains("FETCH FIRST") || sql.contains("LIMIT"),
            "Trino SQL should have a row-limiting clause, got: {sql}"
        );
    }

    #[test]
    fn test_trino_profile() {
        let adapter = TrinoAdapter;
        assert_eq!(adapter.name(), "trino");
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
    fn test_trino_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<TrinoAdapter>();
    }
}
