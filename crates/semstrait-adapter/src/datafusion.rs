//! DataFusion adapter — produces Substrait plans.

use semstrait_core::EngineProfile;
use semstrait_ir::{LogicalPlan, PlanArtifact, SubstraitSerializer};

use crate::{AdaptError, EngineAdapter};

/// DataFusion adapter — produces Substrait plans.
///
/// DataFusion natively consumes Substrait via `datafusion-substrait`,
/// so this adapter serializes `LogicalPlan` -> `substrait::proto::Plan`.
pub struct DataFusionAdapter;

impl EngineProfile for DataFusionAdapter {
    fn name(&self) -> &str {
        "datafusion"
    }
    fn supports_substrait(&self) -> bool {
        true
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

impl EngineAdapter for DataFusionAdapter {
    fn adapt(&self, plan: &LogicalPlan) -> Result<PlanArtifact, AdaptError> {
        let substrait_plan = SubstraitSerializer::to_substrait(plan)
            .map_err(|e| AdaptError::SubstraitSerialization(e.to_string()))?;
        Ok(PlanArtifact::Substrait(Box::new(substrait_plan)))
    }
    // debug_sql() uses default ANSI SQL implementation
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
    fn test_datafusion_adapter_produces_substrait() {
        let adapter = DataFusionAdapter;
        let plan = make_test_plan();
        let artifact = adapter.adapt(&plan).unwrap();
        assert!(
            artifact.is_substrait(),
            "DataFusion adapter should produce Substrait"
        );
        assert!(
            artifact.as_substrait().is_some(),
            "Should be able to access Substrait plan"
        );
    }

    #[test]
    fn test_datafusion_adapter_debug_sql() {
        let adapter = DataFusionAdapter;
        let plan = make_test_plan();
        let sql = adapter.debug_sql(&plan).unwrap();
        assert!(
            sql.contains("SELECT"),
            "debug_sql should produce SQL: {sql}"
        );
        assert!(
            sql.contains("region"),
            "debug_sql should reference 'region': {sql}"
        );
    }

    #[test]
    fn test_datafusion_profile() {
        let adapter = DataFusionAdapter;
        assert_eq!(adapter.name(), "datafusion");
        assert!(adapter.supports_substrait());
        assert!(adapter.supports_window_functions());
        assert!(adapter.supports_full_outer_join());
        assert!(adapter.supports_cte());
        assert!(adapter.supports_subquery());
        assert!(adapter.supports_inline_views());
        assert!(adapter.supports_fetch_rel());
        assert_eq!(adapter.max_join_depth(), Some(10));
    }

    #[test]
    fn test_datafusion_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DataFusionAdapter>();
    }
}
