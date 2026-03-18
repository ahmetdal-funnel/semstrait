//! Tests for semstrait-sql

use crate::dialect::{AnsiDialect, DuckDbDialect, TrinoDialect, SqlDialect};
use crate::emitter::{AnsiSqlEmitter, SqlEmitter};
use crate::expr_renderer::DslExprSqlRenderer;
use semstrait_core::Grain;
use semstrait_ir::{
    AggNode, AggregateMeasure, Aggregation, BinaryOp, DslExpr, FetchNode, FilterNode,
    JoinNode, JoinType, LogicalPlan, NodeMeta, PlanNode, ProjectNode, ScanNode, Schema, SortDirection,
    SortKey, SortNode, UnionNode,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn empty_schema() -> Schema {
    Schema::empty()
}

fn meta() -> NodeMeta {
    NodeMeta::new(empty_schema())
}

fn scan(table: &str, cols: &[&str]) -> PlanNode {
    PlanNode::Scan(ScanNode {
        meta: meta(),
        table_name: table.to_string(),
        projection: cols.iter().map(|c| c.to_string()).collect(),
    })
}

fn plan(root: PlanNode) -> LogicalPlan {
    LogicalPlan::new(root, vec![])
}

// ---------------------------------------------------------------------------
// DslExpr rendering tests
// ---------------------------------------------------------------------------

mod expr_rendering {
    use super::*;

    fn renderer() -> DslExprSqlRenderer<'static> {
        static DIALECT: AnsiDialect = AnsiDialect;
        DslExprSqlRenderer::new(&DIALECT)
    }

    #[test]
    fn column_simple() {
        let r = renderer();
        let expr = DslExpr::Column { name: "id".into(), qualifier: None };
        assert_eq!(r.render(&expr).unwrap(), "\"id\"");
    }

    #[test]
    fn column_qualified() {
        let r = renderer();
        let expr = DslExpr::Column {
            name: "id".into(),
            qualifier: Some("orders".into()),
        };
        assert_eq!(r.render(&expr).unwrap(), "\"orders\".\"id\"");
    }

    #[test]
    fn number_integer() {
        let r = renderer();
        assert_eq!(r.render(&DslExpr::Number(42.0)).unwrap(), "42");
    }

    #[test]
    fn number_float() {
        let r = renderer();
        assert_eq!(r.render(&DslExpr::Number(3.14)).unwrap(), "3.14");
    }

    #[test]
    fn string_literal() {
        let r = renderer();
        assert_eq!(r.render(&DslExpr::StringLit("hello".into())).unwrap(), "'hello'");
    }

    #[test]
    fn string_literal_with_quotes() {
        let r = renderer();
        assert_eq!(
            r.render(&DslExpr::StringLit("it's".into())).unwrap(),
            "'it''s'"
        );
    }

    #[test]
    fn bool_true() {
        let r = renderer();
        assert_eq!(r.render(&DslExpr::Bool(true)).unwrap(), "TRUE");
    }

    #[test]
    fn bool_false() {
        let r = renderer();
        assert_eq!(r.render(&DslExpr::Bool(false)).unwrap(), "FALSE");
    }

    #[test]
    fn null_literal() {
        let r = renderer();
        assert_eq!(r.render(&DslExpr::Null).unwrap(), "NULL");
    }

    #[test]
    fn binary_op_add() {
        let r = renderer();
        let expr = DslExpr::BinaryOp {
            left: Box::new(DslExpr::Column { name: "a".into(), qualifier: None }),
            op: BinaryOp::Add,
            right: Box::new(DslExpr::Number(1.0)),
        };
        assert_eq!(r.render(&expr).unwrap(), "(\"a\" + 1)");
    }

    #[test]
    fn binary_op_eq() {
        let r = renderer();
        let expr = DslExpr::BinaryOp {
            left: Box::new(DslExpr::Column { name: "x".into(), qualifier: None }),
            op: BinaryOp::Eq,
            right: Box::new(DslExpr::Number(10.0)),
        };
        assert_eq!(r.render(&expr).unwrap(), "(\"x\" = 10)");
    }

    #[test]
    fn binary_op_and() {
        let r = renderer();
        let expr = DslExpr::BinaryOp {
            left: Box::new(DslExpr::Bool(true)),
            op: BinaryOp::And,
            right: Box::new(DslExpr::Bool(false)),
        };
        assert_eq!(r.render(&expr).unwrap(), "(TRUE AND FALSE)");
    }

    #[test]
    fn function_call_simple() {
        let r = renderer();
        let expr = DslExpr::FunctionCall {
            name: "COALESCE".into(),
            args: vec![
                DslExpr::Column { name: "x".into(), qualifier: None },
                DslExpr::Number(0.0),
            ],
            distinct: false,
        };
        assert_eq!(r.render(&expr).unwrap(), "COALESCE(\"x\", 0)");
    }

    #[test]
    fn function_call_distinct() {
        let r = renderer();
        let expr = DslExpr::FunctionCall {
            name: "COUNT".into(),
            args: vec![DslExpr::Column { name: "id".into(), qualifier: None }],
            distinct: true,
        };
        assert_eq!(r.render(&expr).unwrap(), "COUNT(DISTINCT \"id\")");
    }

    #[test]
    fn negate() {
        let r = renderer();
        let expr = DslExpr::Negate(Box::new(DslExpr::Number(5.0)));
        assert_eq!(r.render(&expr).unwrap(), "(-5)");
    }

    #[test]
    fn case_expression() {
        let r = renderer();
        let expr = DslExpr::Case {
            when_then: vec![
                (
                    DslExpr::BinaryOp {
                        left: Box::new(DslExpr::Column { name: "status".into(), qualifier: None }),
                        op: BinaryOp::Eq,
                        right: Box::new(DslExpr::StringLit("active".into())),
                    },
                    DslExpr::Number(1.0),
                ),
            ],
            else_expr: Some(Box::new(DslExpr::Number(0.0))),
        };
        assert_eq!(
            r.render(&expr).unwrap(),
            "CASE WHEN (\"status\" = 'active') THEN 1 ELSE 0 END"
        );
    }

    #[test]
    fn case_no_else() {
        let r = renderer();
        let expr = DslExpr::Case {
            when_then: vec![(DslExpr::Bool(true), DslExpr::Number(1.0))],
            else_expr: None,
        };
        assert_eq!(r.render(&expr).unwrap(), "CASE WHEN TRUE THEN 1 END");
    }

    #[test]
    fn aggregate_sum() {
        let r = renderer();
        let measure = AggregateMeasure {
            function: Aggregation::Sum,
            expr: DslExpr::Column { name: "amount".into(), qualifier: None },
            distinct: false,
        };
        assert_eq!(r.render_aggregate(&measure).unwrap(), "SUM(\"amount\")");
    }

    #[test]
    fn aggregate_count_distinct() {
        let r = renderer();
        let measure = AggregateMeasure {
            function: Aggregation::CountDistinct,
            expr: DslExpr::Column { name: "user_id".into(), qualifier: None },
            distinct: false,
        };
        assert_eq!(r.render_aggregate(&measure).unwrap(), "COUNT(DISTINCT \"user_id\")");
    }

    #[test]
    fn aggregate_avg() {
        let r = renderer();
        let measure = AggregateMeasure {
            function: Aggregation::Avg,
            expr: DslExpr::Column { name: "price".into(), qualifier: None },
            distinct: false,
        };
        assert_eq!(r.render_aggregate(&measure).unwrap(), "AVG(\"price\")");
    }

    #[test]
    fn aggregate_min_max() {
        let r = renderer();
        let min_m = AggregateMeasure {
            function: Aggregation::Min,
            expr: DslExpr::Column { name: "ts".into(), qualifier: None },
            distinct: false,
        };
        let max_m = AggregateMeasure {
            function: Aggregation::Max,
            expr: DslExpr::Column { name: "ts".into(), qualifier: None },
            distinct: false,
        };
        assert_eq!(r.render_aggregate(&min_m).unwrap(), "MIN(\"ts\")");
        assert_eq!(r.render_aggregate(&max_m).unwrap(), "MAX(\"ts\")");
    }

    // --- Tests for new DslExpr variants ---

    #[test]
    fn not_expr() {
        let r = renderer();
        let expr = DslExpr::Not(Box::new(DslExpr::Bool(true)));
        assert_eq!(r.render(&expr).unwrap(), "NOT (TRUE)");
    }

    #[test]
    fn not_nested_comparison() {
        let r = renderer();
        let expr = DslExpr::Not(Box::new(DslExpr::BinaryOp {
            left: Box::new(DslExpr::Column { name: "x".into(), qualifier: None }),
            op: BinaryOp::Eq,
            right: Box::new(DslExpr::Number(1.0)),
        }));
        assert_eq!(r.render(&expr).unwrap(), "NOT ((\"x\" = 1))");
    }

    #[test]
    fn is_null() {
        let r = renderer();
        let expr = DslExpr::IsNull(Box::new(DslExpr::Column {
            name: "email".into(),
            qualifier: None,
        }));
        assert_eq!(r.render(&expr).unwrap(), "\"email\" IS NULL");
    }

    #[test]
    fn is_not_null() {
        let r = renderer();
        let expr = DslExpr::IsNotNull(Box::new(DslExpr::Column {
            name: "email".into(),
            qualifier: None,
        }));
        assert_eq!(r.render(&expr).unwrap(), "\"email\" IS NOT NULL");
    }

    #[test]
    fn in_list() {
        let r = renderer();
        let expr = DslExpr::InList {
            expr: Box::new(DslExpr::Column { name: "status".into(), qualifier: None }),
            list: vec![
                DslExpr::StringLit("active".into()),
                DslExpr::StringLit("pending".into()),
            ],
            negated: false,
        };
        assert_eq!(
            r.render(&expr).unwrap(),
            "\"status\" IN ('active', 'pending')"
        );
    }

    #[test]
    fn not_in_list() {
        let r = renderer();
        let expr = DslExpr::InList {
            expr: Box::new(DslExpr::Column { name: "status".into(), qualifier: None }),
            list: vec![
                DslExpr::StringLit("deleted".into()),
                DslExpr::StringLit("archived".into()),
            ],
            negated: true,
        };
        assert_eq!(
            r.render(&expr).unwrap(),
            "\"status\" NOT IN ('deleted', 'archived')"
        );
    }

    #[test]
    fn between() {
        let r = renderer();
        let expr = DslExpr::Between {
            expr: Box::new(DslExpr::Column { name: "age".into(), qualifier: None }),
            low: Box::new(DslExpr::Number(18.0)),
            high: Box::new(DslExpr::Number(65.0)),
            negated: false,
        };
        assert_eq!(r.render(&expr).unwrap(), "\"age\" BETWEEN 18 AND 65");
    }

    #[test]
    fn not_between() {
        let r = renderer();
        let expr = DslExpr::Between {
            expr: Box::new(DslExpr::Column { name: "age".into(), qualifier: None }),
            low: Box::new(DslExpr::Number(18.0)),
            high: Box::new(DslExpr::Number(65.0)),
            negated: true,
        };
        assert_eq!(r.render(&expr).unwrap(), "\"age\" NOT BETWEEN 18 AND 65");
    }

    #[test]
    fn like_pattern() {
        let r = renderer();
        let expr = DslExpr::Like {
            expr: Box::new(DslExpr::Column { name: "name".into(), qualifier: None }),
            pattern: Box::new(DslExpr::StringLit("%smith%".into())),
        };
        assert_eq!(r.render(&expr).unwrap(), "\"name\" LIKE '%smith%'");
    }

    #[test]
    fn coalesce() {
        let r = renderer();
        let expr = DslExpr::Coalesce(vec![
            DslExpr::Column { name: "preferred_name".into(), qualifier: None },
            DslExpr::Column { name: "first_name".into(), qualifier: None },
            DslExpr::StringLit("Unknown".into()),
        ]);
        assert_eq!(
            r.render(&expr).unwrap(),
            "COALESCE(\"preferred_name\", \"first_name\", 'Unknown')"
        );
    }

    #[test]
    fn nullif() {
        let r = renderer();
        let expr = DslExpr::NullIf {
            expr: Box::new(DslExpr::Column { name: "value".into(), qualifier: None }),
            null_expr: Box::new(DslExpr::Number(0.0)),
        };
        assert_eq!(r.render(&expr).unwrap(), "NULLIF(\"value\", 0)");
    }

    #[test]
    fn date_trunc() {
        let r = renderer();
        let expr = DslExpr::DateTrunc {
            grain: "month".into(),
            expr: Box::new(DslExpr::Column { name: "created_at".into(), qualifier: None }),
        };
        assert_eq!(
            r.render(&expr).unwrap(),
            "DATE_TRUNC('month', \"created_at\")"
        );
    }

    #[test]
    fn safe_divide() {
        let r = renderer();
        let expr = DslExpr::BinaryOp {
            left: Box::new(DslExpr::Column { name: "revenue".into(), qualifier: None }),
            op: BinaryOp::SafeDivide,
            right: Box::new(DslExpr::Column { name: "count".into(), qualifier: None }),
        };
        assert_eq!(
            r.render(&expr).unwrap(),
            "(CASE WHEN \"count\" = 0 THEN NULL ELSE \"revenue\" / \"count\" END)"
        );
    }
}

// ---------------------------------------------------------------------------
// Plan → SQL emission tests
// ---------------------------------------------------------------------------

mod plan_emission {
    use super::*;

    fn emitter() -> AnsiSqlEmitter<AnsiDialect> {
        AnsiSqlEmitter::new(AnsiDialect)
    }

    #[test]
    fn scan_simple() {
        let e = emitter();
        let p = plan(scan("orders", &["id", "amount"]));
        let sql = e.emit(&p).unwrap();
        assert_eq!(sql, "SELECT \"id\", \"amount\" FROM \"orders\"");
    }

    #[test]
    fn scan_star() {
        let e = emitter();
        let p = plan(scan("orders", &[]));
        let sql = e.emit(&p).unwrap();
        assert_eq!(sql, "SELECT * FROM \"orders\"");
    }

    #[test]
    fn filter_where() {
        let e = emitter();
        let root = PlanNode::Filter(FilterNode {
            meta: meta(),
            input: Box::new(scan("orders", &["id", "amount"])),
            predicate: DslExpr::BinaryOp {
                left: Box::new(DslExpr::Column { name: "amount".into(), qualifier: None }),
                op: BinaryOp::Gt,
                right: Box::new(DslExpr::Number(100.0)),
            },
        });
        let sql = e.emit(&plan(root)).unwrap();
        assert_eq!(
            sql,
            "SELECT * FROM (SELECT \"id\", \"amount\" FROM \"orders\") AS _f WHERE (\"amount\" > 100)"
        );
    }

    #[test]
    fn project_expressions() {
        let e = emitter();
        let root = PlanNode::Project(ProjectNode {
            meta: meta(),
            input: Box::new(scan("orders", &["id", "amount", "tax"])),
            expressions: vec![
                DslExpr::Column { name: "id".into(), qualifier: None },
                DslExpr::BinaryOp {
                    left: Box::new(DslExpr::Column { name: "amount".into(), qualifier: None }),
                    op: BinaryOp::Add,
                    right: Box::new(DslExpr::Column { name: "tax".into(), qualifier: None }),
                },
            ],
        });
        let sql = e.emit(&plan(root)).unwrap();
        assert_eq!(
            sql,
            "SELECT \"id\", (\"amount\" + \"tax\") FROM (SELECT \"id\", \"amount\", \"tax\" FROM \"orders\") AS _p"
        );
    }

    #[test]
    fn aggregate_group_by() {
        let e = emitter();
        let root = PlanNode::Aggregate(AggNode {
            meta: meta(),
            input: Box::new(scan("orders", &["region", "amount"])),
            group_by: vec![DslExpr::Column { name: "region".into(), qualifier: None }],
            aggregates: vec![AggregateMeasure {
                function: Aggregation::Sum,
                expr: DslExpr::Column { name: "amount".into(), qualifier: None },
                distinct: false,
            }],
        });
        let sql = e.emit(&plan(root)).unwrap();
        assert_eq!(
            sql,
            "SELECT \"region\", SUM(\"amount\") FROM (SELECT \"region\", \"amount\" FROM \"orders\") AS _a GROUP BY \"region\""
        );
    }

    #[test]
    fn aggregate_no_group_by() {
        let e = emitter();
        let root = PlanNode::Aggregate(AggNode {
            meta: meta(),
            input: Box::new(scan("orders", &["amount"])),
            group_by: vec![],
            aggregates: vec![AggregateMeasure {
                function: Aggregation::Count,
                expr: DslExpr::Column { name: "amount".into(), qualifier: None },
                distinct: false,
            }],
        });
        let sql = e.emit(&plan(root)).unwrap();
        assert_eq!(
            sql,
            "SELECT COUNT(\"amount\") FROM (SELECT \"amount\" FROM \"orders\") AS _a"
        );
    }

    #[test]
    fn join_inner() {
        let e = emitter();
        let root = PlanNode::Join(JoinNode {
            meta: meta(),
            left: Box::new(scan("orders", &["id", "customer_id"])),
            right: Box::new(scan("customers", &["id", "name"])),
            join_type: JoinType::Inner,
            condition: DslExpr::BinaryOp {
                left: Box::new(DslExpr::Column {
                    name: "customer_id".into(),
                    qualifier: None,
                }),
                op: BinaryOp::Eq,
                right: Box::new(DslExpr::Column {
                    name: "id".into(),
                    qualifier: None,
                }),
            },
        });
        let sql = e.emit(&plan(root)).unwrap();
        assert_eq!(
            sql,
            "SELECT * FROM (SELECT \"id\", \"customer_id\" FROM \"orders\") AS _l \
             INNER JOIN \
             (SELECT \"id\", \"name\" FROM \"customers\") AS _r \
             ON (\"customer_id\" = \"id\")"
        );
    }

    #[test]
    fn join_left() {
        let e = emitter();
        let root = PlanNode::Join(JoinNode {
            meta: meta(),
            left: Box::new(scan("a", &["x"])),
            right: Box::new(scan("b", &["y"])),
            join_type: JoinType::Left,
            condition: DslExpr::BinaryOp {
                left: Box::new(DslExpr::Column { name: "x".into(), qualifier: None }),
                op: BinaryOp::Eq,
                right: Box::new(DslExpr::Column { name: "y".into(), qualifier: None }),
            },
        });
        let sql = e.emit(&plan(root)).unwrap();
        assert!(sql.contains("LEFT JOIN"));
    }

    #[test]
    fn union_all() {
        let e = emitter();
        let root = PlanNode::Union(UnionNode {
            meta: meta(),
            inputs: vec![
                scan("orders_2023", &["id", "amount"]),
                scan("orders_2024", &["id", "amount"]),
            ],
            distinct: false,
        });
        let sql = e.emit(&plan(root)).unwrap();
        assert_eq!(
            sql,
            "SELECT \"id\", \"amount\" FROM \"orders_2023\" \
             UNION ALL \
             SELECT \"id\", \"amount\" FROM \"orders_2024\""
        );
    }

    #[test]
    fn union_distinct() {
        let e = emitter();
        let root = PlanNode::Union(UnionNode {
            meta: meta(),
            inputs: vec![
                scan("orders_2023", &["id", "amount"]),
                scan("orders_2024", &["id", "amount"]),
            ],
            distinct: true,
        });
        let sql = e.emit(&plan(root)).unwrap();
        assert_eq!(
            sql,
            "SELECT \"id\", \"amount\" FROM \"orders_2023\" \
             UNION DISTINCT \
             SELECT \"id\", \"amount\" FROM \"orders_2024\""
        );
    }

    #[test]
    fn sort_order_by() {
        let e = emitter();
        let root = PlanNode::Sort(SortNode {
            meta: meta(),
            input: Box::new(scan("orders", &["id", "amount"])),
            sort_keys: vec![
                SortKey {
                    expr: DslExpr::Column { name: "amount".into(), qualifier: None },
                    direction: SortDirection::Descending,
                },
                SortKey {
                    expr: DslExpr::Column { name: "id".into(), qualifier: None },
                    direction: SortDirection::Ascending,
                },
            ],
        });
        let sql = e.emit(&plan(root)).unwrap();
        assert_eq!(
            sql,
            "SELECT * FROM (SELECT \"id\", \"amount\" FROM \"orders\") AS _s ORDER BY \"amount\" DESC, \"id\" ASC"
        );
    }

    #[test]
    fn fetch_limit_offset() {
        let e = emitter();
        let root = PlanNode::Fetch(FetchNode {
            meta: meta(),
            input: Box::new(scan("orders", &["id"])),
            count: Some(10),
            offset: 20,
        });
        let sql = e.emit(&plan(root)).unwrap();
        assert_eq!(
            sql,
            "SELECT * FROM (SELECT \"id\" FROM \"orders\") AS _t OFFSET 20 ROWS FETCH FIRST 10 ROWS ONLY"
        );
    }

    #[test]
    fn fetch_limit_only() {
        let e = emitter();
        let root = PlanNode::Fetch(FetchNode {
            meta: meta(),
            input: Box::new(scan("orders", &["id"])),
            count: Some(5),
            offset: 0,
        });
        let sql = e.emit(&plan(root)).unwrap();
        assert_eq!(
            sql,
            "SELECT * FROM (SELECT \"id\" FROM \"orders\") AS _t FETCH FIRST 5 ROWS ONLY"
        );
    }

    #[test]
    fn nested_plan() {
        // Scan → Filter → Aggregate → Sort → Fetch
        let e = emitter();

        let scan_node = scan("events", &["user_id", "event_type", "value"]);

        let filter_node = PlanNode::Filter(FilterNode {
            meta: meta(),
            input: Box::new(scan_node),
            predicate: DslExpr::BinaryOp {
                left: Box::new(DslExpr::Column { name: "event_type".into(), qualifier: None }),
                op: BinaryOp::Eq,
                right: Box::new(DslExpr::StringLit("purchase".into())),
            },
        });

        let agg_node = PlanNode::Aggregate(AggNode {
            meta: meta(),
            input: Box::new(filter_node),
            group_by: vec![DslExpr::Column { name: "user_id".into(), qualifier: None }],
            aggregates: vec![AggregateMeasure {
                function: Aggregation::Sum,
                expr: DslExpr::Column { name: "value".into(), qualifier: None },
                distinct: false,
            }],
        });

        let sort_node = PlanNode::Sort(SortNode {
            meta: meta(),
            input: Box::new(agg_node),
            sort_keys: vec![SortKey {
                expr: DslExpr::Column { name: "user_id".into(), qualifier: None },
                direction: SortDirection::Ascending,
            }],
        });

        let fetch_node = PlanNode::Fetch(FetchNode {
            meta: meta(),
            input: Box::new(sort_node),
            count: Some(100),
            offset: 0,
        });

        let sql = e.emit(&plan(fetch_node)).unwrap();

        // Verify structure: should contain all pieces
        assert!(sql.contains("FROM \"events\""), "should reference events table");
        assert!(sql.contains("WHERE (\"event_type\" = 'purchase')"), "should have WHERE clause");
        assert!(sql.contains("GROUP BY \"user_id\""), "should have GROUP BY");
        assert!(sql.contains("SUM(\"value\")"), "should have SUM aggregate");
        assert!(sql.contains("ORDER BY \"user_id\" ASC"), "should have ORDER BY");
        assert!(sql.contains("FETCH FIRST 100 ROWS ONLY"), "should have FETCH FIRST");
    }
}

// ---------------------------------------------------------------------------
// Dialect-specific tests
// ---------------------------------------------------------------------------

mod dialect_tests {
    use super::*;

    #[test]
    fn ansi_quote_identifier() {
        let d = AnsiDialect;
        assert_eq!(d.quote_identifier("col"), "\"col\"");
        assert_eq!(d.quote_identifier("my\"col"), "\"my\"\"col\"");
    }

    #[test]
    fn duckdb_quote_identifier() {
        let d = DuckDbDialect;
        assert_eq!(d.quote_identifier("col"), "\"col\"");
        assert_eq!(d.quote_identifier("my\"col"), "\"my\"\"col\"");
    }

    #[test]
    fn trino_quote_identifier() {
        let d = TrinoDialect;
        assert_eq!(d.quote_identifier("col"), "\"col\"");
    }

    #[test]
    fn ansi_date_trunc() {
        let d = AnsiDialect;
        assert_eq!(d.date_trunc(&Grain::Day, "\"ts\""), "DATE_TRUNC('day', \"ts\")");
        assert_eq!(d.date_trunc(&Grain::Month, "\"ts\""), "DATE_TRUNC('month', \"ts\")");
    }

    #[test]
    fn duckdb_date_trunc() {
        let d = DuckDbDialect;
        assert_eq!(d.date_trunc(&Grain::Week, "\"ts\""), "date_trunc('week', \"ts\")");
    }

    #[test]
    fn trino_date_trunc() {
        let d = TrinoDialect;
        assert_eq!(d.date_trunc(&Grain::Quarter, "\"ts\""), "date_trunc('quarter', \"ts\")");
    }

    #[test]
    fn null_safe_eq() {
        let d = AnsiDialect;
        assert_eq!(d.null_safe_eq("a", "b"), "(a IS NOT DISTINCT FROM b)");
    }

    #[test]
    fn current_timestamp() {
        assert_eq!(AnsiDialect.current_timestamp(), "CURRENT_TIMESTAMP");
        assert_eq!(DuckDbDialect.current_timestamp(), "current_timestamp");
        assert_eq!(TrinoDialect.current_timestamp(), "current_timestamp");
    }

    #[test]
    fn window_row_number() {
        let d = AnsiDialect;
        assert_eq!(
            d.window_row_number(&["\"a\"", "\"b\""], "\"c\" ASC"),
            "ROW_NUMBER() OVER (PARTITION BY \"a\", \"b\" ORDER BY \"c\" ASC)"
        );
        assert_eq!(
            d.window_row_number(&[], "\"c\" DESC"),
            "ROW_NUMBER() OVER (ORDER BY \"c\" DESC)"
        );
    }

    #[test]
    fn duckdb_emitter_scan() {
        let e = AnsiSqlEmitter::new(DuckDbDialect);
        let p = plan(scan("orders", &["id", "amount"]));
        let sql = e.emit(&p).unwrap();
        assert_eq!(sql, "SELECT \"id\", \"amount\" FROM \"orders\"");
    }

    #[test]
    fn trino_emitter_scan() {
        let e = AnsiSqlEmitter::new(TrinoDialect);
        let p = plan(scan("orders", &["id", "amount"]));
        let sql = e.emit(&p).unwrap();
        assert_eq!(sql, "SELECT \"id\", \"amount\" FROM \"orders\"");
    }

    #[test]
    fn duckdb_emitter_filter() {
        let e = AnsiSqlEmitter::new(DuckDbDialect);
        let root = PlanNode::Filter(FilterNode {
            meta: meta(),
            input: Box::new(scan("t", &["a"])),
            predicate: DslExpr::BinaryOp {
                left: Box::new(DslExpr::Column { name: "a".into(), qualifier: None }),
                op: BinaryOp::Gt,
                right: Box::new(DslExpr::Number(5.0)),
            },
        });
        let sql = e.emit(&plan(root)).unwrap();
        assert!(sql.contains("\"a\""));
        assert!(sql.contains("WHERE (\"a\" > 5)"));
    }

    #[test]
    fn supports_cte() {
        assert!(AnsiDialect.supports_cte());
        assert!(DuckDbDialect.supports_cte());
        assert!(TrinoDialect.supports_cte());
    }
}

// ---------------------------------------------------------------------------
// Union with empty inputs should fail
// ---------------------------------------------------------------------------

#[test]
fn union_empty_inputs_errors() {
    let e = AnsiSqlEmitter::new(AnsiDialect);
    let root = PlanNode::Union(UnionNode {
        meta: meta(),
        inputs: vec![],
        distinct: false,
    });
    let result = e.emit(&plan(root));
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Polyglot emitter tests (feature-gated)
// ---------------------------------------------------------------------------

#[cfg(feature = "polyglot")]
mod polyglot_tests {
    use super::*;
    use crate::dialect::TargetDialect;
    use crate::polyglot_emitter::PolyglotEmitter;

    fn polyglot(target: TargetDialect) -> PolyglotEmitter {
        PolyglotEmitter::new(target)
    }

    #[test]
    fn ansi_passthrough() {
        let e = polyglot(TargetDialect::Ansi);
        let p = plan(scan("orders", &["id", "amount"]));
        let sql = e.emit(&p).unwrap();
        assert_eq!(sql, "SELECT \"id\", \"amount\" FROM \"orders\"");
    }

    #[test]
    fn duckdb_limit_conversion() {
        let e = polyglot(TargetDialect::DuckDb);
        let root = PlanNode::Fetch(FetchNode {
            meta: meta(),
            input: Box::new(scan("orders", &["id"])),
            count: Some(10),
            offset: 0,
        });
        let sql = e.emit(&plan(root)).unwrap();
        assert!(sql.contains("LIMIT 10"), "DuckDB should use LIMIT, got: {sql}");
        assert!(!sql.contains("FETCH FIRST"), "DuckDB should not use FETCH FIRST, got: {sql}");
    }

    #[test]
    fn trino_keeps_fetch_first() {
        let e = polyglot(TargetDialect::Trino);
        let root = PlanNode::Fetch(FetchNode {
            meta: meta(),
            input: Box::new(scan("orders", &["id"])),
            count: Some(10),
            offset: 0,
        });
        let sql = e.emit(&plan(root)).unwrap();
        assert!(
            sql.contains("FETCH FIRST 10 ROWS ONLY"),
            "Trino should use FETCH FIRST, got: {sql}"
        );
    }

    #[test]
    fn spark_backtick_quoting() {
        let e = polyglot(TargetDialect::Spark);
        let p = plan(scan("orders", &["id", "amount"]));
        let sql = e.emit(&p).unwrap();
        assert!(sql.contains("`id`"), "Spark should use backtick quoting, got: {sql}");
        assert!(sql.contains("`orders`"), "Spark should use backtick quoting, got: {sql}");
    }

    #[test]
    fn databricks_backtick_quoting() {
        let e = polyglot(TargetDialect::Databricks);
        let p = plan(scan("orders", &["id", "amount"]));
        let sql = e.emit(&p).unwrap();
        assert!(sql.contains("`id`"), "Databricks should use backtick quoting, got: {sql}");
    }

    #[test]
    fn snowflake_double_quote() {
        let e = polyglot(TargetDialect::Snowflake);
        let p = plan(scan("orders", &["id", "amount"]));
        let sql = e.emit(&p).unwrap();
        assert!(sql.contains("\"id\""), "Snowflake should use double-quote, got: {sql}");
    }

    #[test]
    fn datafusion_keeps_fetch_first() {
        let e = polyglot(TargetDialect::DataFusion);
        let root = PlanNode::Fetch(FetchNode {
            meta: meta(),
            input: Box::new(scan("orders", &["id"])),
            count: Some(5),
            offset: 0,
        });
        let sql = e.emit(&plan(root)).unwrap();
        assert!(
            sql.contains("FETCH FIRST 5 ROWS ONLY"),
            "DataFusion should use FETCH FIRST, got: {sql}"
        );
    }

    #[test]
    fn postgresql_keeps_fetch_first() {
        let e = polyglot(TargetDialect::PostgreSql);
        let root = PlanNode::Fetch(FetchNode {
            meta: meta(),
            input: Box::new(scan("orders", &["id"])),
            count: Some(5),
            offset: 0,
        });
        let sql = e.emit(&plan(root)).unwrap();
        assert!(
            sql.contains("FETCH FIRST 5 ROWS ONLY"),
            "PostgreSQL should use FETCH FIRST, got: {sql}"
        );
    }

    #[test]
    fn complex_nested_plan_duckdb() {
        let e = polyglot(TargetDialect::DuckDb);
        let scan_node = scan("events", &["user_id", "event_type", "value"]);
        let filter_node = PlanNode::Filter(FilterNode {
            meta: meta(),
            input: Box::new(scan_node),
            predicate: DslExpr::BinaryOp {
                left: Box::new(DslExpr::Column { name: "event_type".into(), qualifier: None }),
                op: BinaryOp::Eq,
                right: Box::new(DslExpr::StringLit("purchase".into())),
            },
        });
        let agg_node = PlanNode::Aggregate(AggNode {
            meta: meta(),
            input: Box::new(filter_node),
            group_by: vec![DslExpr::Column { name: "user_id".into(), qualifier: None }],
            aggregates: vec![AggregateMeasure {
                function: Aggregation::Sum,
                expr: DslExpr::Column { name: "value".into(), qualifier: None },
                distinct: false,
            }],
        });
        let sort_node = PlanNode::Sort(SortNode {
            meta: meta(),
            input: Box::new(agg_node),
            sort_keys: vec![SortKey {
                expr: DslExpr::Column { name: "user_id".into(), qualifier: None },
                direction: SortDirection::Ascending,
            }],
        });
        let fetch_node = PlanNode::Fetch(FetchNode {
            meta: meta(),
            input: Box::new(sort_node),
            count: Some(100),
            offset: 0,
        });
        let sql = e.emit(&plan(fetch_node)).unwrap();
        assert!(sql.contains("LIMIT 100"), "DuckDB should use LIMIT, got: {sql}");
        assert!(sql.contains("SUM(\"value\")"), "should have aggregate, got: {sql}");
        assert!(sql.contains("GROUP BY"), "should have GROUP BY, got: {sql}");
        assert!(sql.contains("ORDER BY"), "should have ORDER BY, got: {sql}");
    }

    #[test]
    fn complex_nested_plan_spark() {
        let e = polyglot(TargetDialect::Spark);
        let scan_node = scan("events", &["user_id", "value"]);
        let agg_node = PlanNode::Aggregate(AggNode {
            meta: meta(),
            input: Box::new(scan_node),
            group_by: vec![DslExpr::Column { name: "user_id".into(), qualifier: None }],
            aggregates: vec![AggregateMeasure {
                function: Aggregation::Sum,
                expr: DslExpr::Column { name: "value".into(), qualifier: None },
                distinct: false,
            }],
        });
        let fetch_node = PlanNode::Fetch(FetchNode {
            meta: meta(),
            input: Box::new(agg_node),
            count: Some(10),
            offset: 0,
        });
        let sql = e.emit(&plan(fetch_node)).unwrap();
        assert!(sql.contains("`user_id`"), "Spark should use backtick, got: {sql}");
        assert!(sql.contains("`value`"), "Spark should use backtick, got: {sql}");
        assert!(sql.contains("LIMIT 10"), "Spark should use LIMIT, got: {sql}");
    }

    #[test]
    fn union_all_preserved() {
        let e = polyglot(TargetDialect::DuckDb);
        let root = PlanNode::Union(UnionNode {
            meta: meta(),
            inputs: vec![
                scan("orders_2023", &["id", "amount"]),
                scan("orders_2024", &["id", "amount"]),
            ],
            distinct: false,
        });
        let sql = e.emit(&plan(root)).unwrap();
        assert!(sql.contains("UNION ALL"), "should preserve UNION ALL, got: {sql}");
    }

    #[test]
    fn full_outer_join_preserved() {
        let e = polyglot(TargetDialect::Spark);
        let root = PlanNode::Join(JoinNode {
            meta: meta(),
            left: Box::new(scan("a", &["id", "x"])),
            right: Box::new(scan("b", &["id", "y"])),
            join_type: JoinType::Full,
            condition: DslExpr::BinaryOp {
                left: Box::new(DslExpr::Column { name: "x".into(), qualifier: None }),
                op: BinaryOp::Eq,
                right: Box::new(DslExpr::Column { name: "y".into(), qualifier: None }),
            },
        });
        let sql = e.emit(&plan(root)).unwrap();
        assert!(
            sql.contains("FULL OUTER JOIN"),
            "should preserve FULL OUTER JOIN, got: {sql}"
        );
    }

    #[test]
    fn case_when_and_safe_divide_across_dialects() {
        for target in [TargetDialect::DuckDb, TargetDialect::Trino, TargetDialect::Spark] {
            let e = polyglot(target);
            let root = PlanNode::Project(ProjectNode {
                meta: meta(),
                input: Box::new(scan("orders", &["revenue", "count"])),
                expressions: vec![DslExpr::BinaryOp {
                    left: Box::new(DslExpr::Column { name: "revenue".into(), qualifier: None }),
                    op: BinaryOp::SafeDivide,
                    right: Box::new(DslExpr::Column { name: "count".into(), qualifier: None }),
                }],
            });
            let sql = e.emit(&plan(root)).unwrap();
            assert!(sql.contains("CASE WHEN"), "SafeDivide should emit CASE WHEN for {target:?}, got: {sql}");
        }
    }
}
