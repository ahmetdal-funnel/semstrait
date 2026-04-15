//! DataFusion adapter — produces Substrait plans.

mod plan_builder;

use plan_builder::DataFusionPlanBuilder;
use semstrait_ir::{
    FunctionRegistry, LogicalPlan, PlanArtifact, PlanBuilder,
    SubstraitSerializer,
};

use crate::sql::{AnsiSqlEmitter, DataFusionDialect, SqlEmitter};
use crate::{AdaptError, EngineAdapter};

/// DataFusion adapter — produces Substrait plans.
///
/// DataFusion natively consumes Substrait via `datafusion-substrait`,
/// so this adapter serializes `LogicalPlan` -> `substrait::proto::Plan`.
pub struct DataFusionAdapter;

impl EngineAdapter for DataFusionAdapter {
    fn name(&self) -> &str {
        "datafusion"
    }

    fn plan_builder(&self) -> Option<Box<dyn PlanBuilder>> {
        Some(Box::new(DataFusionPlanBuilder::new()))
    }

    fn adapt(&self, plan: &LogicalPlan) -> Result<PlanArtifact, AdaptError> {
        let registry = FunctionRegistry::datafusion();
        let substrait_plan = SubstraitSerializer::to_substrait(plan, &registry)
            .map_err(|e| AdaptError::SubstraitSerialization(e.to_string()))?;
        Ok(PlanArtifact::Substrait(Box::new(substrait_plan)))
    }
    fn debug_sql(&self, plan: &LogicalPlan) -> Result<String, AdaptError> {
        let emitter = AnsiSqlEmitter::new(DataFusionDialect);
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
    fn test_datafusion_name() {
        let adapter = DataFusionAdapter;
        assert_eq!(adapter.name(), "datafusion");
    }

    #[test]
    fn test_datafusion_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DataFusionAdapter>();
    }
}
