//! Bucketed dimension compilation.
//!
//! Converts a `BucketedDimension` definition into a CASE WHEN expression
//! that maps a numeric column into named bucket strings.

use crate::planner::ir::expr::{BinaryOperator, Column, Expr, Literal};
use crate::schema::model::{Bucket, BucketedDimension};

/// Compile a bucketed dimension into a CASE WHEN expression.
///
/// Generates:
/// ```sql
/// CASE WHEN start <= col AND col < end THEN 'bucket_name'
///      WHEN ... THEN ...
///      ELSE NULL END
/// ```
pub fn compile_bucketed(
    dimension: &BucketedDimension,
    table_alias: &str,
) -> Expr {
    let col = Expr::Column(Column::new(table_alias, &dimension.column));
    let when_then: Vec<(Expr, Expr)> = dimension
        .buckets
        .iter()
        .map(|bucket| {
            let condition = bucket_condition(&col, bucket);
            let result = Expr::Literal(Literal::String(bucket.name.clone()));
            (condition, result)
        })
        .collect();

    Expr::Case {
        when_then,
        else_result: Some(Box::new(Expr::Literal(Literal::Null("string".into())))),
    }
}

/// Build the condition `start <= col AND col < end` for a single bucket.
fn bucket_condition(col: &Expr, bucket: &Bucket) -> Expr {
    let ge_start = Expr::BinaryOp {
        left: Box::new(Expr::Literal(Literal::Float(bucket.start))),
        op: BinaryOperator::LtEq,
        right: Box::new(col.clone()),
    };
    let lt_end = Expr::BinaryOp {
        left: Box::new(col.clone()),
        op: BinaryOperator::Lt,
        right: Box::new(Expr::Literal(Literal::Float(bucket.end))),
    };
    Expr::And(vec![ge_start, lt_end])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dim() -> BucketedDimension {
        BucketedDimension {
            column: "price".into(),
            buckets: vec![
                Bucket { name: "low".into(), start: 0.0, end: 100.0 },
                Bucket { name: "medium".into(), start: 100.0, end: 500.0 },
                Bucket { name: "high".into(), start: 500.0, end: 10000.0 },
            ],
        }
    }

    #[test]
    fn test_compile_bucketed_structure() {
        let expr = compile_bucketed(&test_dim(), "orders");
        match &expr {
            Expr::Case { when_then, else_result } => {
                assert_eq!(when_then.len(), 3);
                assert!(else_result.is_some());
            }
            _ => panic!("expected CASE expression"),
        }
    }

    #[test]
    fn test_compile_bucketed_display() {
        let expr = compile_bucketed(&test_dim(), "orders");
        let s = expr.to_string();
        assert!(s.contains("CASE"));
        assert!(s.contains("WHEN"));
        assert!(s.contains("'low'"));
        assert!(s.contains("'medium'"));
        assert!(s.contains("'high'"));
        assert!(s.contains("END"));
    }

    #[test]
    fn test_single_bucket() {
        let dim = BucketedDimension {
            column: "age".into(),
            buckets: vec![Bucket { name: "adult".into(), start: 18.0, end: 200.0 }],
        };
        let expr = compile_bucketed(&dim, "users");
        let s = expr.to_string();
        assert!(s.contains("'adult'"));
    }
}
