//! Tests for the sql module (dialect, emitter, expr_renderer).

use super::dialect::{AnsiDialect, SqlDialect};
#[cfg(feature = "duckdb")]
use super::dialect::DuckDbDialect;
#[cfg(feature = "spark")]
use super::dialect::SparkDialect;
use super::emitter::{AnsiSqlEmitter, SqlEmitter};
use super::expr_renderer::ExprSqlRenderer;
use semstrait_core::expr::{
    BinaryExpr, ColumnRef, InListExpr, Literal, BetweenExpr, LikeExpr,
    CoalesceExpr, NullIfExpr, DateTruncExpr, CaseExpr, WhenClause, UnaryExpr,
    FunctionCallExpr,
};
use semstrait_core::Grain;
use semstrait_ir::{
    AggNode, AggregateMeasure, Aggregation, BinaryOp, Expr, FetchNode, FilterNode,
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
        location: None,
        format: None,
        projection: cols.iter().map(|c| c.to_string()).collect(),
    })
}

fn plan(root: PlanNode) -> LogicalPlan {
    LogicalPlan::new(root, vec![])
}

// --- Expression construction helpers ---

fn col(name: &str) -> Expr {
    Expr::Column(ColumnRef { name: name.into(), qualifier: None })
}

fn qcol(qualifier: &str, name: &str) -> Expr {
    Expr::Column(ColumnRef { name: name.into(), qualifier: Some(qualifier.into()) })
}

fn int(v: i64) -> Expr {
    Expr::Literal(Literal::Integer { value: v })
}

fn float(v: f64) -> Expr {
    Expr::Literal(Literal::Float { value: v })
}

fn str_lit(s: &str) -> Expr {
    Expr::Literal(Literal::String { value: s.into() })
}

fn bool_lit(b: bool) -> Expr {
    Expr::Literal(Literal::Boolean { value: b })
}

fn null_lit() -> Expr {
    Expr::Literal(Literal::Null)
}

fn bin(left: Expr, op: BinaryOp, right: Expr) -> Expr {
    Expr::BinaryOp(BinaryExpr {
        left: Box::new(left),
        op,
        right: Box::new(right),
    })
}

// ---------------------------------------------------------------------------
// Expr rendering tests
// ---------------------------------------------------------------------------

mod expr_rendering {
    use super::*;

    fn renderer() -> ExprSqlRenderer<'static> {
        static DIALECT: AnsiDialect = AnsiDialect;
        ExprSqlRenderer::new(&DIALECT)
    }

    #[test]
    fn column_simple() {
        let r = renderer();
        assert_eq!(r.render(&col("id")).unwrap(), "\"id\"");
    }

    #[test]
    fn column_qualified() {
        let r = renderer();
        assert_eq!(r.render(&qcol("orders", "id")).unwrap(), "\"orders\".\"id\"");
    }

    #[test]
    fn number_integer() {
        let r = renderer();
        assert_eq!(r.render(&int(42)).unwrap(), "42");
    }

    #[test]
    fn number_float() {
        let r = renderer();
        assert_eq!(r.render(&float(2.72)).unwrap(), "2.72");
    }

    #[test]
    fn string_literal() {
        let r = renderer();
        assert_eq!(r.render(&str_lit("hello")).unwrap(), "'hello'");
    }

    #[test]
    fn string_literal_with_quotes() {
        let r = renderer();
        assert_eq!(r.render(&str_lit("it's")).unwrap(), "'it''s'");
    }

    #[test]
    fn bool_true() {
        let r = renderer();
        assert_eq!(r.render(&bool_lit(true)).unwrap(), "TRUE");
    }

    #[test]
    fn bool_false() {
        let r = renderer();
        assert_eq!(r.render(&bool_lit(false)).unwrap(), "FALSE");
    }

    #[test]
    fn null_literal() {
        let r = renderer();
        assert_eq!(r.render(&null_lit()).unwrap(), "NULL");
    }

    #[test]
    fn binary_op_add() {
        let r = renderer();
        let expr = bin(col("a"), BinaryOp::Add, int(1));
        assert_eq!(r.render(&expr).unwrap(), "(\"a\" + 1)");
    }

    #[test]
    fn binary_op_eq() {
        let r = renderer();
        let expr = bin(col("x"), BinaryOp::Eq, int(10));
        assert_eq!(r.render(&expr).unwrap(), "(\"x\" = 10)");
    }

    #[test]
    fn binary_op_and() {
        let r = renderer();
        let expr = bin(bool_lit(true), BinaryOp::And, bool_lit(false));
        assert_eq!(r.render(&expr).unwrap(), "(TRUE AND FALSE)");
    }

    #[test]
    fn function_call_simple() {
        let r = renderer();
        let expr = Expr::FunctionCall(FunctionCallExpr {
            name: "COALESCE".into(),
            args: vec![col("x"), int(0)],
            distinct: false,
        });
        assert_eq!(r.render(&expr).unwrap(), "COALESCE(\"x\", 0)");
    }

    #[test]
    fn function_call_distinct() {
        let r = renderer();
        let expr = Expr::FunctionCall(FunctionCallExpr {
            name: "COUNT".into(),
            args: vec![col("id")],
            distinct: true,
        });
        assert_eq!(r.render(&expr).unwrap(), "COUNT(DISTINCT \"id\")");
    }

    #[test]
    fn negate() {
        let r = renderer();
        let expr = Expr::Negate(UnaryExpr { expr: Box::new(int(5)) });
        assert_eq!(r.render(&expr).unwrap(), "(-5)");
    }

    #[test]
    fn case_expression() {
        let r = renderer();
        let expr = Expr::Case(CaseExpr {
            when_then: vec![
                WhenClause {
                    condition: bin(col("status"), BinaryOp::Eq, str_lit("active")),
                    result: int(1),
                },
            ],
            else_expr: Some(Box::new(int(0))),
        });
        assert_eq!(
            r.render(&expr).unwrap(),
            "CASE WHEN (\"status\" = 'active') THEN 1 ELSE 0 END"
        );
    }

    #[test]
    fn case_no_else() {
        let r = renderer();
        let expr = Expr::Case(CaseExpr {
            when_then: vec![WhenClause {
                condition: bool_lit(true),
                result: int(1),
            }],
            else_expr: None,
        });
        assert_eq!(r.render(&expr).unwrap(), "CASE WHEN TRUE THEN 1 END");
    }

    #[test]
    fn aggregate_sum() {
        let r = renderer();
        let measure = AggregateMeasure {
            function: Aggregation::Sum,
            expr: col("amount"),
            distinct: false,
            data_type: semstrait_core::DataType::Number,
        };
        assert_eq!(r.render_aggregate(&measure).unwrap(), "SUM(\"amount\")");
    }

    #[test]
    fn aggregate_count_distinct() {
        let r = renderer();
        let measure = AggregateMeasure {
            function: Aggregation::CountDistinct,
            expr: col("user_id"),
            distinct: false,
            data_type: semstrait_core::DataType::Integer,
        };
        assert_eq!(r.render_aggregate(&measure).unwrap(), "COUNT(DISTINCT \"user_id\")");
    }

    #[test]
    fn aggregate_avg() {
        let r = renderer();
        let measure = AggregateMeasure {
            function: Aggregation::Avg,
            expr: col("price"),
            distinct: false,
            data_type: semstrait_core::DataType::Number,
        };
        assert_eq!(r.render_aggregate(&measure).unwrap(), "AVG(\"price\")");
    }

    #[test]
    fn aggregate_min_max() {
        let r = renderer();
        let min_m = AggregateMeasure {
            function: Aggregation::Min,
            expr: col("ts"),
            distinct: false,
            data_type: semstrait_core::DataType::Timestamp { precision: 6 },
        };
        let max_m = AggregateMeasure {
            function: Aggregation::Max,
            expr: col("ts"),
            distinct: false,
            data_type: semstrait_core::DataType::Timestamp { precision: 6 },
        };
        assert_eq!(r.render_aggregate(&min_m).unwrap(), "MIN(\"ts\")");
        assert_eq!(r.render_aggregate(&max_m).unwrap(), "MAX(\"ts\")");
    }

    // --- Tests for new Expr variants ---

    #[test]
    fn not_expr() {
        let r = renderer();
        let expr = Expr::Not(UnaryExpr { expr: Box::new(bool_lit(true)) });
        assert_eq!(r.render(&expr).unwrap(), "NOT (TRUE)");
    }

    #[test]
    fn not_nested_comparison() {
        let r = renderer();
        let expr = Expr::Not(UnaryExpr {
            expr: Box::new(bin(col("x"), BinaryOp::Eq, int(1))),
        });
        assert_eq!(r.render(&expr).unwrap(), "NOT ((\"x\" = 1))");
    }

    #[test]
    fn is_null() {
        let r = renderer();
        let expr = Expr::IsNull(UnaryExpr { expr: Box::new(col("email")) });
        assert_eq!(r.render(&expr).unwrap(), "\"email\" IS NULL");
    }

    #[test]
    fn is_not_null() {
        let r = renderer();
        let expr = Expr::IsNotNull(UnaryExpr { expr: Box::new(col("email")) });
        assert_eq!(r.render(&expr).unwrap(), "\"email\" IS NOT NULL");
    }

    #[test]
    fn in_list() {
        let r = renderer();
        let expr = Expr::InList(InListExpr {
            expr: Box::new(col("status")),
            list: vec![str_lit("active"), str_lit("pending")],
            negated: false,
        });
        assert_eq!(
            r.render(&expr).unwrap(),
            "\"status\" IN ('active', 'pending')"
        );
    }

    #[test]
    fn not_in_list() {
        let r = renderer();
        let expr = Expr::InList(InListExpr {
            expr: Box::new(col("status")),
            list: vec![str_lit("deleted"), str_lit("archived")],
            negated: true,
        });
        assert_eq!(
            r.render(&expr).unwrap(),
            "\"status\" NOT IN ('deleted', 'archived')"
        );
    }

    #[test]
    fn between() {
        let r = renderer();
        let expr = Expr::Between(BetweenExpr {
            expr: Box::new(col("age")),
            low: Box::new(int(18)),
            high: Box::new(int(65)),
            negated: false,
        });
        assert_eq!(r.render(&expr).unwrap(), "\"age\" BETWEEN 18 AND 65");
    }

    #[test]
    fn not_between() {
        let r = renderer();
        let expr = Expr::Between(BetweenExpr {
            expr: Box::new(col("age")),
            low: Box::new(int(18)),
            high: Box::new(int(65)),
            negated: true,
        });
        assert_eq!(r.render(&expr).unwrap(), "\"age\" NOT BETWEEN 18 AND 65");
    }

    #[test]
    fn like_pattern() {
        let r = renderer();
        let expr = Expr::Like(LikeExpr {
            expr: Box::new(col("name")),
            pattern: Box::new(str_lit("%smith%")),
        });
        assert_eq!(r.render(&expr).unwrap(), "\"name\" LIKE '%smith%'");
    }

    #[test]
    fn ilike_ansi_default() {
        let r = renderer(); // ANSI dialect
        let expr = Expr::ilike(col("name"), str_lit("%Smith%"));
        // ANSI default: LOWER fallback
        assert_eq!(
            r.render(&expr).unwrap(),
            "LOWER(\"name\") LIKE LOWER('%Smith%')"
        );
    }

    #[test]
    fn regexp_match_ansi_substring() {
        let r = renderer();
        let expr = Expr::regexp_match(col("email"), str_lit("@example\\.com"), false);
        assert_eq!(
            r.render(&expr).unwrap(),
            "REGEXP_LIKE(\"email\", '@example\\.com')"
        );
    }

    #[test]
    fn regexp_match_ansi_full() {
        let r = renderer();
        let expr = Expr::regexp_match(col("email"), str_lit(".*@example\\.com"), true);
        assert_eq!(
            r.render(&expr).unwrap(),
            "REGEXP_LIKE(\"email\", CONCAT('^', '.*@example\\.com', '$'))"
        );
    }

    #[test]
    fn regexp_extract_ansi() {
        let r = renderer();
        let expr = Expr::regexp_extract(col("url"), str_lit("https?://([^/]+)"), 1);
        assert_eq!(
            r.render(&expr).unwrap(),
            "REGEXP_EXTRACT(\"url\", 'https?://([^/]+)', 1)"
        );
    }

    #[test]
    fn coalesce() {
        let r = renderer();
        let expr = Expr::Coalesce(CoalesceExpr {
            exprs: vec![col("preferred_name"), col("first_name"), str_lit("Unknown")],
        });
        assert_eq!(
            r.render(&expr).unwrap(),
            "COALESCE(\"preferred_name\", \"first_name\", 'Unknown')"
        );
    }

    #[test]
    fn nullif() {
        let r = renderer();
        let expr = Expr::NullIf(NullIfExpr {
            expr: Box::new(col("value")),
            null_expr: Box::new(int(0)),
        });
        assert_eq!(r.render(&expr).unwrap(), "NULLIF(\"value\", 0)");
    }

    #[test]
    fn date_trunc() {
        let r = renderer();
        let expr = Expr::DateTrunc(DateTruncExpr {
            grain: Grain::Month,
            expr: Box::new(col("created_at")),
        });
        assert_eq!(
            r.render(&expr).unwrap(),
            "DATE_TRUNC('month', \"created_at\")"
        );
    }

    #[test]
    fn safe_divide() {
        let r = renderer();
        let expr = bin(col("revenue"), BinaryOp::SafeDivide, col("count"));
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
            predicate: bin(col("amount"), BinaryOp::Gt, int(100)),
        });
        let sql = e.emit(&plan(root)).unwrap();
        assert_eq!(
            sql,
            "SELECT * FROM (SELECT \"id\", \"amount\" FROM \"orders\") AS _f0 WHERE (\"amount\" > 100)"
        );
    }

    #[test]
    fn project_expressions() {
        let e = emitter();
        let root = PlanNode::Project(ProjectNode {
            meta: meta(),
            input: Box::new(scan("orders", &["id", "amount", "tax"])),
            expressions: vec![
                col("id"),
                bin(col("amount"), BinaryOp::Add, col("tax")),
            ],
        });
        let sql = e.emit(&plan(root)).unwrap();
        assert_eq!(
            sql,
            "SELECT \"id\", (\"amount\" + \"tax\") FROM (SELECT \"id\", \"amount\", \"tax\" FROM \"orders\") AS _p0"
        );
    }

    #[test]
    fn aggregate_group_by() {
        let e = emitter();
        let root = PlanNode::Aggregate(AggNode {
            meta: meta(),
            input: Box::new(scan("orders", &["region", "amount"])),
            group_by: vec![col("region")],
            aggregates: vec![AggregateMeasure {
                function: Aggregation::Sum,
                expr: col("amount"),
                distinct: false,
                data_type: semstrait_core::DataType::Number,
            }],
        });
        let sql = e.emit(&plan(root)).unwrap();
        assert_eq!(
            sql,
            "SELECT \"region\", SUM(\"amount\") FROM (SELECT \"region\", \"amount\" FROM \"orders\") AS _a0 GROUP BY \"region\""
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
                expr: col("amount"),
                distinct: false,
                data_type: semstrait_core::DataType::Integer,
            }],
        });
        let sql = e.emit(&plan(root)).unwrap();
        assert_eq!(
            sql,
            "SELECT COUNT(\"amount\") FROM (SELECT \"amount\" FROM \"orders\") AS _a0"
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
            condition: bin(col("customer_id"), BinaryOp::Eq, col("id")),
        });
        let sql = e.emit(&plan(root)).unwrap();
        assert_eq!(
            sql,
            "SELECT * FROM (SELECT \"id\", \"customer_id\" FROM \"orders\") AS _j0 \
             INNER JOIN \
             (SELECT \"id\", \"name\" FROM \"customers\") AS _j1 \
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
            condition: bin(col("x"), BinaryOp::Eq, col("y")),
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
                    expr: col("amount"),
                    direction: SortDirection::Descending,
                },
                SortKey {
                    expr: col("id"),
                    direction: SortDirection::Ascending,
                },
            ],
        });
        let sql = e.emit(&plan(root)).unwrap();
        assert_eq!(
            sql,
            "SELECT * FROM (SELECT \"id\", \"amount\" FROM \"orders\") AS _s0 ORDER BY \"amount\" DESC, \"id\" ASC"
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
            "SELECT * FROM (SELECT \"id\" FROM \"orders\") AS _t0 OFFSET 20 ROWS FETCH FIRST 10 ROWS ONLY"
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
            "SELECT * FROM (SELECT \"id\" FROM \"orders\") AS _t0 FETCH FIRST 5 ROWS ONLY"
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
            predicate: bin(col("event_type"), BinaryOp::Eq, str_lit("purchase")),
        });

        let agg_node = PlanNode::Aggregate(AggNode {
            meta: meta(),
            input: Box::new(filter_node),
            group_by: vec![col("user_id")],
            aggregates: vec![AggregateMeasure {
                function: Aggregation::Sum,
                expr: col("value"),
                distinct: false,
                data_type: semstrait_core::DataType::Number,
            }],
        });

        let sort_node = PlanNode::Sort(SortNode {
            meta: meta(),
            input: Box::new(agg_node),
            sort_keys: vec![SortKey {
                expr: col("user_id"),
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

    #[cfg(feature = "duckdb")]
    #[test]
    fn duckdb_quote_identifier() {
        let d = DuckDbDialect;
        assert_eq!(d.quote_identifier("col"), "\"col\"");
        assert_eq!(d.quote_identifier("my\"col"), "\"my\"\"col\"");
    }

    #[cfg(feature = "spark")]
    #[test]
    fn spark_quote_identifier() {
        let d = SparkDialect;
        assert_eq!(d.quote_identifier("col"), "\"col\"");
    }

    #[test]
    fn ansi_date_trunc() {
        let d = AnsiDialect;
        assert_eq!(d.date_trunc(&Grain::Day, "\"ts\""), "DATE_TRUNC('day', \"ts\")");
        assert_eq!(d.date_trunc(&Grain::Month, "\"ts\""), "DATE_TRUNC('month', \"ts\")");
    }

    #[cfg(feature = "duckdb")]
    #[test]
    fn duckdb_date_trunc() {
        let d = DuckDbDialect;
        assert_eq!(d.date_trunc(&Grain::Week, "\"ts\""), "date_trunc('week', \"ts\")");
    }

    #[cfg(feature = "spark")]
    #[test]
    fn spark_date_trunc() {
        let d = SparkDialect;
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
    }

    #[cfg(feature = "duckdb")]
    #[test]
    fn current_timestamp_duckdb() {
        assert_eq!(DuckDbDialect.current_timestamp(), "current_timestamp");
    }

    #[cfg(feature = "spark")]
    #[test]
    fn current_timestamp_spark() {
        assert_eq!(SparkDialect.current_timestamp(), "current_timestamp()");
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

    #[cfg(feature = "duckdb")]
    #[test]
    fn duckdb_emitter_scan() {
        let e = AnsiSqlEmitter::new(DuckDbDialect);
        let p = plan(scan("orders", &["id", "amount"]));
        let sql = e.emit(&p).unwrap();
        assert_eq!(sql, "SELECT \"id\", \"amount\" FROM \"orders\"");
    }

    #[cfg(feature = "spark")]
    #[test]
    fn spark_emitter_scan() {
        let e = AnsiSqlEmitter::new(SparkDialect);
        let p = plan(scan("orders", &["id", "amount"]));
        let sql = e.emit(&p).unwrap();
        assert_eq!(sql, "SELECT \"id\", \"amount\" FROM \"orders\"");
    }

    #[cfg(feature = "duckdb")]
    #[test]
    fn duckdb_emitter_filter() {
        let e = AnsiSqlEmitter::new(DuckDbDialect);
        let root = PlanNode::Filter(FilterNode {
            meta: meta(),
            input: Box::new(scan("t", &["a"])),
            predicate: bin(col("a"), BinaryOp::Gt, int(5)),
        });
        let sql = e.emit(&plan(root)).unwrap();
        assert!(sql.contains("\"a\""));
        assert!(sql.contains("WHERE (\"a\" > 5)"));
    }

    #[cfg(feature = "spark")]
    #[test]
    fn spark_emitter_limit() {
        let e = AnsiSqlEmitter::new(SparkDialect);
        let root = PlanNode::Fetch(FetchNode {
            meta: meta(),
            input: Box::new(scan("orders", &["id"])),
            count: Some(10),
            offset: 0,
        });
        let sql = e.emit(&plan(root)).unwrap();
        assert!(sql.contains("LIMIT 10"), "Spark should use LIMIT, got: {sql}");
        assert!(!sql.contains("FETCH FIRST"), "Spark should NOT use FETCH FIRST, got: {sql}");
    }

    #[test]
    fn supports_cte() {
        assert!(AnsiDialect.supports_cte());
    }

    // -- ILIKE dialect tests --

    #[cfg(feature = "duckdb")]
    #[test]
    fn duckdb_ilike_native() {
        let r = ExprSqlRenderer::new(&DuckDbDialect);
        let expr = Expr::ilike(col("name"), str_lit("%test%"));
        assert_eq!(r.render(&expr).unwrap(), "\"name\" ILIKE '%test%'");
    }

    #[cfg(feature = "spark")]
    #[test]
    fn spark_ilike_lowered() {
        let r = ExprSqlRenderer::new(&SparkDialect);
        let expr = Expr::ilike(col("name"), str_lit("%Test%"));
        assert_eq!(
            r.render(&expr).unwrap(),
            "LOWER(\"name\") LIKE LOWER('%Test%')"
        );
    }

    // -- RegexpMatch dialect tests --

    #[cfg(feature = "duckdb")]
    #[test]
    fn duckdb_regexp_match_substring() {
        let r = ExprSqlRenderer::new(&DuckDbDialect);
        let expr = Expr::regexp_match(col("email"), str_lit("@example"), false);
        assert_eq!(
            r.render(&expr).unwrap(),
            "regexp_matches(\"email\", '@example')"
        );
    }

    #[cfg(feature = "duckdb")]
    #[test]
    fn duckdb_regexp_match_full() {
        let r = ExprSqlRenderer::new(&DuckDbDialect);
        let expr = Expr::regexp_match(col("email"), str_lit(".*@example\\.com"), true);
        assert_eq!(
            r.render(&expr).unwrap(),
            "regexp_matches(\"email\", CONCAT('^', '.*@example\\.com', '$'))"
        );
    }

    #[cfg(feature = "spark")]
    #[test]
    fn spark_regexp_match_full() {
        let r = ExprSqlRenderer::new(&SparkDialect);
        let expr = Expr::regexp_match(col("code"), str_lit("^[A-Z]{3}$"), true);
        assert_eq!(
            r.render(&expr).unwrap(),
            "\"code\" RLIKE '^[A-Z]{3}$'"
        );
    }

    #[cfg(feature = "spark")]
    #[test]
    fn spark_regexp_match_substring() {
        let r = ExprSqlRenderer::new(&SparkDialect);
        let expr = Expr::regexp_match(col("code"), str_lit("[0-9]+"), false);
        assert_eq!(
            r.render(&expr).unwrap(),
            "\"code\" RLIKE CONCAT('.*', '[0-9]+', '.*')"
        );
    }

    // -- RegexpExtract dialect tests --

    #[cfg(feature = "duckdb")]
    #[test]
    fn duckdb_regexp_extract() {
        let r = ExprSqlRenderer::new(&DuckDbDialect);
        let expr = Expr::regexp_extract(col("url"), str_lit("https?://([^/]+)"), 1);
        assert_eq!(
            r.render(&expr).unwrap(),
            "regexp_extract(\"url\", 'https?://([^/]+)', 1)"
        );
    }

    #[cfg(feature = "spark")]
    #[test]
    fn spark_regexp_extract() {
        let r = ExprSqlRenderer::new(&SparkDialect);
        let expr = Expr::regexp_extract(col("path"), str_lit("/(\\d+)/"), 1);
        assert_eq!(
            r.render(&expr).unwrap(),
            "regexp_extract(\"path\", '/(\\d+)/', 1)"
        );
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
// type_name() dialect tests — see docs/DATATYPE_CATALOG.md
// ---------------------------------------------------------------------------

mod type_name_tests {
    use super::*;
    use semstrait_core::DataType;

    // ── ANSI defaults ─────────────────────────────────────────────

    #[test]
    fn ansi_type_names() {
        let d = AnsiDialect;
        assert_eq!(d.type_name(&DataType::Integer), "INTEGER");
        assert_eq!(d.type_name(&DataType::Number), "DOUBLE PRECISION");
        assert_eq!(d.type_name(&DataType::Decimal { precision: 10, scale: 2 }), "DECIMAL(10,2)");
        assert_eq!(d.type_name(&DataType::String), "VARCHAR");
        assert_eq!(d.type_name(&DataType::Boolean), "BOOLEAN");
        assert_eq!(d.type_name(&DataType::Date), "DATE");
        assert_eq!(d.type_name(&DataType::Timestamp { precision: 6 }), "TIMESTAMP(6)");
        assert_eq!(d.type_name(&DataType::Binary), "VARBINARY");
    }

    // ── DataFusion ────────────────────────────────────────────────

    #[cfg(feature = "datafusion")]
    mod datafusion {
        use super::*;
        use crate::sql::dialect::DataFusionDialect;

        #[test]
        fn integer_is_bigint() {
            assert_eq!(DataFusionDialect.type_name(&DataType::Integer), "BIGINT");
        }

        #[test]
        fn number_is_double() {
            assert_eq!(DataFusionDialect.type_name(&DataType::Number), "DOUBLE");
        }

        #[test]
        fn string_is_varchar() {
            assert_eq!(DataFusionDialect.type_name(&DataType::String), "VARCHAR");
        }

        #[test]
        fn boolean_is_boolean() {
            assert_eq!(DataFusionDialect.type_name(&DataType::Boolean), "BOOLEAN");
        }

        #[test]
        fn date_is_date() {
            assert_eq!(DataFusionDialect.type_name(&DataType::Date), "DATE");
        }

        #[test]
        fn timestamp_precision() {
            assert_eq!(DataFusionDialect.type_name(&DataType::Timestamp { precision: 0 }), "TIMESTAMP(0)");
            assert_eq!(DataFusionDialect.type_name(&DataType::Timestamp { precision: 3 }), "TIMESTAMP(3)");
            assert_eq!(DataFusionDialect.type_name(&DataType::Timestamp { precision: 6 }), "TIMESTAMP(6)");
            assert_eq!(DataFusionDialect.type_name(&DataType::Timestamp { precision: 9 }), "TIMESTAMP(9)");
        }

        #[test]
        fn binary_is_bytea() {
            assert_eq!(DataFusionDialect.type_name(&DataType::Binary), "BYTEA");
        }

        #[test]
        fn decimal_unchanged() {
            assert_eq!(DataFusionDialect.type_name(&DataType::Decimal { precision: 18, scale: 2 }), "DECIMAL(18,2)");
        }
    }

    // ── DuckDB ────────────────────────────────────────────────────

    #[cfg(feature = "duckdb")]
    mod duckdb {
        use super::*;
        use crate::sql::dialect::DuckDbDialect;

        #[test]
        fn integer_is_bigint() {
            assert_eq!(DuckDbDialect.type_name(&DataType::Integer), "BIGINT");
        }

        #[test]
        fn number_is_double() {
            assert_eq!(DuckDbDialect.type_name(&DataType::Number), "DOUBLE");
        }

        #[test]
        fn string_is_varchar() {
            assert_eq!(DuckDbDialect.type_name(&DataType::String), "VARCHAR");
        }

        #[test]
        fn boolean_is_boolean() {
            assert_eq!(DuckDbDialect.type_name(&DataType::Boolean), "BOOLEAN");
        }

        #[test]
        fn date_is_date() {
            assert_eq!(DuckDbDialect.type_name(&DataType::Date), "DATE");
        }

        #[test]
        fn timestamp_seconds() {
            assert_eq!(DuckDbDialect.type_name(&DataType::Timestamp { precision: 0 }), "TIMESTAMP_S");
        }

        #[test]
        fn timestamp_milliseconds() {
            assert_eq!(DuckDbDialect.type_name(&DataType::Timestamp { precision: 3 }), "TIMESTAMP_MS");
        }

        #[test]
        fn timestamp_microseconds() {
            assert_eq!(DuckDbDialect.type_name(&DataType::Timestamp { precision: 6 }), "TIMESTAMP");
        }

        #[test]
        fn timestamp_nanoseconds() {
            assert_eq!(DuckDbDialect.type_name(&DataType::Timestamp { precision: 9 }), "TIMESTAMP_NS");
        }

        #[test]
        fn binary_is_blob() {
            assert_eq!(DuckDbDialect.type_name(&DataType::Binary), "BLOB");
        }

        #[test]
        fn timestamp_nonstandard_precision_falls_back() {
            // Non-standard precision (not 0/3/6/9) defensively maps to bare TIMESTAMP (microseconds)
            assert_eq!(DuckDbDialect.type_name(&DataType::Timestamp { precision: 2 }), "TIMESTAMP");
        }

        #[test]
        fn decimal_unchanged() {
            assert_eq!(DuckDbDialect.type_name(&DataType::Decimal { precision: 38, scale: 10 }), "DECIMAL(38,10)");
        }
    }

    // ── Spark ─────────────────────────────────────────────────────

    #[cfg(feature = "spark")]
    mod spark {
        use super::*;
        use crate::sql::dialect::SparkDialect;

        #[test]
        fn integer_is_bigint() {
            assert_eq!(SparkDialect.type_name(&DataType::Integer), "BIGINT");
        }

        #[test]
        fn number_is_double() {
            assert_eq!(SparkDialect.type_name(&DataType::Number), "DOUBLE");
        }

        #[test]
        fn string_is_string() {
            assert_eq!(SparkDialect.type_name(&DataType::String), "STRING");
        }

        #[test]
        fn boolean_is_boolean() {
            assert_eq!(SparkDialect.type_name(&DataType::Boolean), "BOOLEAN");
        }

        #[test]
        fn date_is_date() {
            assert_eq!(SparkDialect.type_name(&DataType::Date), "DATE");
        }

        #[test]
        fn timestamp_no_precision() {
            // Spark TIMESTAMP has no precision parameter — always bare TIMESTAMP
            assert_eq!(SparkDialect.type_name(&DataType::Timestamp { precision: 0 }), "TIMESTAMP");
            assert_eq!(SparkDialect.type_name(&DataType::Timestamp { precision: 3 }), "TIMESTAMP");
            assert_eq!(SparkDialect.type_name(&DataType::Timestamp { precision: 6 }), "TIMESTAMP");
            assert_eq!(SparkDialect.type_name(&DataType::Timestamp { precision: 9 }), "TIMESTAMP");
        }

        #[test]
        fn binary_is_binary() {
            assert_eq!(SparkDialect.type_name(&DataType::Binary), "BINARY");
        }

        #[test]
        fn decimal_unchanged() {
            assert_eq!(SparkDialect.type_name(&DataType::Decimal { precision: 10, scale: 0 }), "DECIMAL(10,0)");
        }
    }
}

// ---------------------------------------------------------------------------
// Polyglot emitter tests (feature-gated)
// ---------------------------------------------------------------------------

#[cfg(any(feature = "duckdb", feature = "spark"))]
mod polyglot_tests {
    use super::*;
    use super::super::dialect::TargetDialect;
    use super::super::polyglot_emitter::PolyglotEmitter;

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
            predicate: bin(col("event_type"), BinaryOp::Eq, str_lit("purchase")),
        });
        let agg_node = PlanNode::Aggregate(AggNode {
            meta: meta(),
            input: Box::new(filter_node),
            group_by: vec![col("user_id")],
            aggregates: vec![AggregateMeasure {
                function: Aggregation::Sum,
                expr: col("value"),
                distinct: false,
                data_type: semstrait_core::DataType::Number,
            }],
        });
        let sort_node = PlanNode::Sort(SortNode {
            meta: meta(),
            input: Box::new(agg_node),
            sort_keys: vec![SortKey {
                expr: col("user_id"),
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
            group_by: vec![col("user_id")],
            aggregates: vec![AggregateMeasure {
                function: Aggregation::Sum,
                expr: col("value"),
                distinct: false,
                data_type: semstrait_core::DataType::Number,
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
            condition: bin(col("x"), BinaryOp::Eq, col("y")),
        });
        let sql = e.emit(&plan(root)).unwrap();
        assert!(
            sql.contains("FULL OUTER JOIN"),
            "should preserve FULL OUTER JOIN, got: {sql}"
        );
    }

    #[test]
    fn case_when_and_safe_divide_across_dialects() {
        for target in [TargetDialect::DuckDb, TargetDialect::Spark, TargetDialect::Snowflake] {
            let e = polyglot(target);
            let root = PlanNode::Project(ProjectNode {
                meta: meta(),
                input: Box::new(scan("orders", &["revenue", "count"])),
                expressions: vec![bin(col("revenue"), BinaryOp::SafeDivide, col("count"))],
            });
            let sql = e.emit(&plan(root)).unwrap();
            assert!(sql.contains("CASE WHEN"), "SafeDivide should emit CASE WHEN for {target:?}, got: {sql}");
        }
    }
}
