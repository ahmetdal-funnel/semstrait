//! Human-readable tree display for LogicalPlan and PlanNode.
//!
//! Produces indented output similar to DataFusion's EXPLAIN:
//!
//! ```text
//! Projection: region, SUM(amount) AS revenue
//!   Aggregate: groupBy=[region], aggr=[SUM(amount)]
//!     Filter: status = 'active'
//!       TableScan: orders [region, amount, status]
//! ```

use super::logical::LogicalPlan;
use super::node::*;
use std::fmt;

impl fmt::Display for LogicalPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_node(&self.root, f, 0)
    }
}

impl fmt::Display for PlanNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_node(self, f, 0)
    }
}

fn indent(f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
    for _ in 0..depth {
        write!(f, "  ")?;
    }
    Ok(())
}

fn fmt_node(node: &PlanNode, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
    indent(f, depth)?;
    match node {
        PlanNode::Scan(n) => fmt_scan(n, f),
        PlanNode::Filter(n) => {
            fmt_filter(n, f)?;
            writeln!(f)?;
            fmt_node(&n.input, f, depth + 1)
        }
        PlanNode::Project(n) => {
            fmt_project(n, f)?;
            writeln!(f)?;
            fmt_node(&n.input, f, depth + 1)
        }
        PlanNode::Aggregate(n) => {
            fmt_aggregate(n, f)?;
            writeln!(f)?;
            fmt_node(&n.input, f, depth + 1)
        }
        PlanNode::Join(n) => {
            fmt_join(n, f)?;
            writeln!(f)?;
            fmt_node(&n.left, f, depth + 1)?;
            writeln!(f)?;
            fmt_node(&n.right, f, depth + 1)
        }
        PlanNode::Union(n) => {
            fmt_union(n, f)?;
            for (i, input) in n.inputs.iter().enumerate() {
                writeln!(f)?;
                fmt_node(input, f, depth + 1)?;
                // No trailing newline after last input
                if i < n.inputs.len() - 1 {
                    // handled by next iteration's writeln
                }
            }
            Ok(())
        }
        PlanNode::Sort(n) => {
            fmt_sort(n, f)?;
            writeln!(f)?;
            fmt_node(&n.input, f, depth + 1)
        }
        PlanNode::Fetch(n) => {
            fmt_fetch(n, f)?;
            writeln!(f)?;
            fmt_node(&n.input, f, depth + 1)
        }
    }
}

fn fmt_scan(n: &ScanNode, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "TableScan: {} [{}]", n.table_name, n.projection.join(", "))
}

fn fmt_filter(n: &FilterNode, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "Filter: {}", n.predicate)
}

fn fmt_project(n: &ProjectNode, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let exprs: Vec<String> = n.expressions.iter().map(|e| e.to_string()).collect();
    write!(f, "Projection: {}", exprs.join(", "))
}

fn fmt_aggregate(n: &AggNode, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let groups: Vec<String> = n.group_by.iter().map(|e| e.to_string()).collect();
    let aggs: Vec<String> = n.aggregates.iter().map(|a| {
        let prefix = if a.distinct { "DISTINCT " } else { "" };
        format!("{}({}{})", a.function.sql_name(), prefix, a.expr)
    }).collect();
    write!(f, "Aggregate: groupBy=[{}], aggr=[{}]", groups.join(", "), aggs.join(", "))
}

fn fmt_join(n: &JoinNode, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let jtype = match n.join_type {
        JoinType::Inner => "INNER",
        JoinType::Left => "LEFT",
        JoinType::Right => "RIGHT",
        JoinType::Full => "FULL",
    };
    write!(f, "Join: {} ON {}", jtype, n.condition)
}

fn fmt_union(n: &UnionNode, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let mode = if n.distinct { "DISTINCT" } else { "ALL" };
    write!(f, "Union: {} ({} inputs)", mode, n.inputs.len())
}

fn fmt_sort(n: &SortNode, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let keys: Vec<String> = n.sort_keys.iter().map(|k| {
        let dir = match k.direction {
            SortDirection::Ascending => "ASC",
            SortDirection::Descending => "DESC",
        };
        format!("{} {}", k.expr, dir)
    }).collect();
    write!(f, "Sort: [{}]", keys.join(", "))
}

fn fmt_fetch(n: &FetchNode, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match (n.count, n.offset) {
        (Some(count), 0) => write!(f, "Limit: {}", count),
        (Some(count), offset) => write!(f, "Limit: {}, offset={}", count, offset),
        (None, offset) => write!(f, "Offset: {}", offset),
    }
}

#[cfg(test)]
mod tests {
    use super::super::meta::NodeMeta;
    use super::*;
    use crate::schema::Schema;

    fn meta() -> NodeMeta {
        NodeMeta::new(Schema::empty())
    }

    #[test]
    fn test_simple_scan() {
        let node = PlanNode::Scan(ScanNode {
            meta: meta(),
            table_name: "orders".into(),
            location: None,
            format: None,
            projection: vec!["region".into(), "amount".into()],
        });
        let output = node.to_string();
        assert_eq!(output, "TableScan: orders [region, amount]");
    }

    #[test]
    fn test_aggregate_over_scan() {
        let scan = PlanNode::Scan(ScanNode {
            meta: meta(),
            table_name: "orders".into(),
            location: None,
            format: None,
            projection: vec!["region".into(), "amount".into()],
        });
        let agg = PlanNode::Aggregate(AggNode {
            meta: meta(),
            input: Box::new(scan),
            group_by: vec![Expr::column("region")],
            aggregates: vec![AggregateMeasure {
                function: Aggregation::Sum,
                expr: Expr::column("amount"),
                distinct: false,
                data_type: DataType::Number,
            }],
        });
        let output = agg.to_string();
        assert_eq!(
            output,
            "Aggregate: groupBy=[region], aggr=[SUM(amount)]\n  TableScan: orders [region, amount]"
        );
    }

    #[test]
    fn test_filter_with_projection() {
        let scan = PlanNode::Scan(ScanNode {
            meta: meta(),
            table_name: "orders".into(),
            location: None,
            format: None,
            projection: vec!["region".into(), "amount".into(), "status".into()],
        });
        let filter = PlanNode::Filter(FilterNode {
            meta: meta(),
            input: Box::new(scan),
            predicate: Expr::eq(Expr::column("status"), Expr::string("active")),
        });
        let proj = PlanNode::Project(ProjectNode {
            meta: meta(),
            input: Box::new(filter),
            expressions: vec![Expr::column("region"), Expr::column("amount")],
        });
        let output = proj.to_string();
        let expected = "\
Projection: region, amount
  Filter: status = 'active'
    TableScan: orders [region, amount, status]";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_join_tree() {
        let left = PlanNode::Scan(ScanNode {
            meta: meta(),
            table_name: "orders".into(),
            location: None,
            format: None,
            projection: vec!["id".into(), "amount".into()],
        });
        let right = PlanNode::Scan(ScanNode {
            meta: meta(),
            table_name: "customers".into(),
            location: None,
            format: None,
            projection: vec!["id".into(), "name".into()],
        });
        let join = PlanNode::Join(JoinNode {
            meta: meta(),
            left: Box::new(left),
            right: Box::new(right),
            join_type: JoinType::Inner,
            condition: Expr::eq(
                Expr::qualified_column("orders", "id"),
                Expr::qualified_column("customers", "id"),
            ),
        });
        let output = join.to_string();
        let expected = "\
Join: INNER ON orders.id = customers.id
  TableScan: orders [id, amount]
  TableScan: customers [id, name]";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_union() {
        let scan_us = PlanNode::Scan(ScanNode {
            meta: meta(),
            table_name: "orders_us".into(),
            location: None,
            format: None,
            projection: vec!["region".into(), "amount".into()],
        });
        let scan_eu = PlanNode::Scan(ScanNode {
            meta: meta(),
            table_name: "orders_eu".into(),
            location: None,
            format: None,
            projection: vec!["region".into(), "amount".into()],
        });
        let union = PlanNode::Union(UnionNode {
            meta: meta(),
            inputs: vec![scan_us, scan_eu],
            distinct: false,
        });
        let output = union.to_string();
        let expected = "\
Union: ALL (2 inputs)
  TableScan: orders_us [region, amount]
  TableScan: orders_eu [region, amount]";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_sort_and_limit() {
        let scan = PlanNode::Scan(ScanNode {
            meta: meta(),
            table_name: "orders".into(),
            location: None,
            format: None,
            projection: vec!["region".into(), "amount".into()],
        });
        let sort = PlanNode::Sort(SortNode {
            meta: meta(),
            input: Box::new(scan),
            sort_keys: vec![
                SortKey {
                    expr: Expr::column("amount"),
                    direction: SortDirection::Descending,
                },
            ],
        });
        let fetch = PlanNode::Fetch(FetchNode {
            meta: meta(),
            input: Box::new(sort),
            count: Some(10),
            offset: 0,
        });
        let output = fetch.to_string();
        let expected = "\
Limit: 10
  Sort: [amount DESC]
    TableScan: orders [region, amount]";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_complex_plan_with_join_and_aggregate() {
        // SELECT region, SUM(amount) FROM orders JOIN customers ON ... GROUP BY region
        let orders = PlanNode::Scan(ScanNode {
            meta: meta(),
            table_name: "orders".into(),
            location: None,
            format: None,
            projection: vec!["customer_id".into(), "amount".into()],
        });
        let customers = PlanNode::Scan(ScanNode {
            meta: meta(),
            table_name: "customers".into(),
            location: None,
            format: None,
            projection: vec!["id".into(), "region".into()],
        });
        let join = PlanNode::Join(JoinNode {
            meta: meta(),
            left: Box::new(orders),
            right: Box::new(customers),
            join_type: JoinType::Left,
            condition: Expr::eq(Expr::column("customer_id"), Expr::column("id")),
        });
        let agg = PlanNode::Aggregate(AggNode {
            meta: meta(),
            input: Box::new(join),
            group_by: vec![Expr::column("region")],
            aggregates: vec![AggregateMeasure {
                function: Aggregation::Sum,
                expr: Expr::column("amount"),
                distinct: false,
                data_type: DataType::Number,
            }],
        });
        let output = agg.to_string();
        let expected = "\
Aggregate: groupBy=[region], aggr=[SUM(amount)]
  Join: LEFT ON customer_id = id
    TableScan: orders [customer_id, amount]
    TableScan: customers [id, region]";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_logical_plan_display() {
        let scan = PlanNode::Scan(ScanNode {
            meta: meta(),
            table_name: "orders".into(),
            location: None,
            format: None,
            projection: vec!["region".into(), "amount".into()],
        });
        let plan = LogicalPlan::new(scan, vec!["region".into(), "amount".into()]);
        let output = plan.to_string();
        assert_eq!(output, "TableScan: orders [region, amount]");
    }

    #[test]
    fn test_union_with_aggregates() {
        // Grainset-style: UNION ALL of two aggregated scans
        let make_branch = |table: &str| -> PlanNode {
            let scan = PlanNode::Scan(ScanNode {
                meta: meta(),
                table_name: table.into(),
                location: None,
                format: None,
                projection: vec!["region".into(), "amount".into()],
            });
            PlanNode::Aggregate(AggNode {
                meta: meta(),
                input: Box::new(scan),
                group_by: vec![Expr::column("region")],
                aggregates: vec![AggregateMeasure {
                    function: Aggregation::Sum,
                    expr: Expr::column("amount"),
                    distinct: false,
                    data_type: DataType::Number,
                }],
            })
        };
        let union = PlanNode::Union(UnionNode {
            meta: meta(),
            inputs: vec![make_branch("orders_us"), make_branch("orders_eu")],
            distinct: false,
        });
        let output = union.to_string();
        let expected = "\
Union: ALL (2 inputs)
  Aggregate: groupBy=[region], aggr=[SUM(amount)]
    TableScan: orders_us [region, amount]
  Aggregate: groupBy=[region], aggr=[SUM(amount)]
    TableScan: orders_eu [region, amount]";
        assert_eq!(output, expected);
    }
}
