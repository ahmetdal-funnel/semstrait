//! DSL expression tree — the ONLY way to express computations in v1.
//! Raw SQL strings are rejected at compile time.

use crate::grain::Grain;
use serde::{Deserialize, Serialize};

/// DSL expression tree — the ONLY way to express computations in v1.
/// Raw SQL strings are rejected at compile time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DslExpr {
    /// Reference to a column by name.
    Column(ColumnExpr),

    /// Literal value.
    Literal(LiteralExpr),

    /// Reference to an entity (measure, metric, dimension) by name.
    EntityRef(EntityRefExpr),

    /// Aggregation functions.
    Sum(AggExpr),
    Count(AggExpr),
    CountDistinct(AggExpr),
    Avg(AggExpr),
    Min(AggExpr),
    Max(AggExpr),

    /// Arithmetic operations.
    Add(BinaryExpr),
    Subtract(BinaryExpr),
    Multiply(BinaryExpr),
    Divide(BinaryExpr),
    SafeDivide(BinaryExpr),
    Negate(UnaryExpr),

    /// Comparison operations.
    Eq(BinaryExpr),
    Ne(BinaryExpr),
    Gt(BinaryExpr),
    Gte(BinaryExpr),
    Lt(BinaryExpr),
    Lte(BinaryExpr),

    /// List and range operations.
    InList(InListExpr),
    Between(BetweenExpr),
    Like(BinaryExpr),

    /// Null checks.
    IsNull(UnaryExpr),
    IsNotNull(UnaryExpr),

    /// Logical operations.
    And(LogicalExpr),
    Or(LogicalExpr),
    Not(UnaryExpr),

    /// Conditional expressions.
    Case(CaseExpr),
    Coalesce(CoalesceExpr),
    NullIf(BinaryExpr),

    /// Date/time functions.
    DateTrunc(DateTruncExpr),

    /// Guard expression: CASE WHEN condition THEN expr END.
    /// Used for measure filters in multi-measure aggregation context.
    Guard(GuardExpr),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnExpr {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LiteralExpr {
    Integer { value: i64 },
    Float { value: f64 },
    String { value: String },
    Boolean { value: bool },
    Null,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityRefExpr {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggExpr {
    pub expr: Box<DslExpr>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BinaryExpr {
    pub left: Box<DslExpr>,
    pub right: Box<DslExpr>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnaryExpr {
    pub expr: Box<DslExpr>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InListExpr {
    pub expr: Box<DslExpr>,
    pub list: Vec<DslExpr>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetweenExpr {
    pub expr: Box<DslExpr>,
    pub lower: Box<DslExpr>,
    pub upper: Box<DslExpr>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogicalExpr {
    pub exprs: Vec<DslExpr>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaseExpr {
    pub when: Vec<WhenClause>,
    pub else_expr: Option<Box<DslExpr>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WhenClause {
    pub condition: DslExpr,
    pub result: DslExpr,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoalesceExpr {
    pub exprs: Vec<DslExpr>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DateTruncExpr {
    pub grain: Grain,
    pub expr: Box<DslExpr>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuardExpr {
    pub condition: Box<DslExpr>,
    pub expr: Box<DslExpr>,
}

// Convenience constructors
impl DslExpr {
    /// Create a column reference.
    pub fn column(name: impl Into<String>) -> Self {
        DslExpr::Column(ColumnExpr { name: name.into() })
    }

    /// Create an entity reference.
    pub fn entity_ref(name: impl Into<String>) -> Self {
        DslExpr::EntityRef(EntityRefExpr { name: name.into() })
    }

    /// Create an integer literal.
    pub fn int(value: i64) -> Self {
        DslExpr::Literal(LiteralExpr::Integer { value })
    }

    /// Create a float literal.
    pub fn float(value: f64) -> Self {
        DslExpr::Literal(LiteralExpr::Float { value })
    }

    /// Create a string literal.
    pub fn string(value: impl Into<String>) -> Self {
        DslExpr::Literal(LiteralExpr::String {
            value: value.into(),
        })
    }

    /// Create a boolean literal.
    pub fn bool(value: bool) -> Self {
        DslExpr::Literal(LiteralExpr::Boolean { value })
    }

    /// Create a null literal.
    pub fn null() -> Self {
        DslExpr::Literal(LiteralExpr::Null)
    }

    /// Create a sum aggregation.
    pub fn sum(expr: DslExpr) -> Self {
        DslExpr::Sum(AggExpr {
            expr: Box::new(expr),
        })
    }

    /// Create a count aggregation.
    pub fn count(expr: DslExpr) -> Self {
        DslExpr::Count(AggExpr {
            expr: Box::new(expr),
        })
    }

    /// Create a count distinct aggregation.
    pub fn count_distinct(expr: DslExpr) -> Self {
        DslExpr::CountDistinct(AggExpr {
            expr: Box::new(expr),
        })
    }

    /// Create an avg aggregation.
    pub fn avg(expr: DslExpr) -> Self {
        DslExpr::Avg(AggExpr {
            expr: Box::new(expr),
        })
    }

    /// Create a min aggregation.
    pub fn min(expr: DslExpr) -> Self {
        DslExpr::Min(AggExpr {
            expr: Box::new(expr),
        })
    }

    /// Create a max aggregation.
    pub fn max(expr: DslExpr) -> Self {
        DslExpr::Max(AggExpr {
            expr: Box::new(expr),
        })
    }

    /// Create an addition expression.
    pub fn add(left: DslExpr, right: DslExpr) -> Self {
        DslExpr::Add(BinaryExpr {
            left: Box::new(left),
            right: Box::new(right),
        })
    }

    /// Create a subtraction expression.
    pub fn subtract(left: DslExpr, right: DslExpr) -> Self {
        DslExpr::Subtract(BinaryExpr {
            left: Box::new(left),
            right: Box::new(right),
        })
    }

    /// Create a multiplication expression.
    pub fn multiply(left: DslExpr, right: DslExpr) -> Self {
        DslExpr::Multiply(BinaryExpr {
            left: Box::new(left),
            right: Box::new(right),
        })
    }

    /// Create a division expression.
    pub fn divide(left: DslExpr, right: DslExpr) -> Self {
        DslExpr::Divide(BinaryExpr {
            left: Box::new(left),
            right: Box::new(right),
        })
    }

    /// Create a safe division expression (handles division by zero).
    pub fn safe_divide(left: DslExpr, right: DslExpr) -> Self {
        DslExpr::SafeDivide(BinaryExpr {
            left: Box::new(left),
            right: Box::new(right),
        })
    }

    /// Create a negation expression.
    pub fn negate(expr: DslExpr) -> Self {
        DslExpr::Negate(UnaryExpr {
            expr: Box::new(expr),
        })
    }

    /// Create an equality comparison.
    pub fn eq(left: DslExpr, right: DslExpr) -> Self {
        DslExpr::Eq(BinaryExpr {
            left: Box::new(left),
            right: Box::new(right),
        })
    }

    /// Create a not-equal comparison.
    pub fn ne(left: DslExpr, right: DslExpr) -> Self {
        DslExpr::Ne(BinaryExpr {
            left: Box::new(left),
            right: Box::new(right),
        })
    }

    /// Create a greater-than comparison.
    pub fn gt(left: DslExpr, right: DslExpr) -> Self {
        DslExpr::Gt(BinaryExpr {
            left: Box::new(left),
            right: Box::new(right),
        })
    }

    /// Create a greater-than-or-equal comparison.
    pub fn gte(left: DslExpr, right: DslExpr) -> Self {
        DslExpr::Gte(BinaryExpr {
            left: Box::new(left),
            right: Box::new(right),
        })
    }

    /// Create a less-than comparison.
    pub fn lt(left: DslExpr, right: DslExpr) -> Self {
        DslExpr::Lt(BinaryExpr {
            left: Box::new(left),
            right: Box::new(right),
        })
    }

    /// Create a less-than-or-equal comparison.
    pub fn lte(left: DslExpr, right: DslExpr) -> Self {
        DslExpr::Lte(BinaryExpr {
            left: Box::new(left),
            right: Box::new(right),
        })
    }

    /// Create an AND logical expression.
    pub fn and(exprs: Vec<DslExpr>) -> Self {
        DslExpr::And(LogicalExpr { exprs })
    }

    /// Create an OR logical expression.
    pub fn or(exprs: Vec<DslExpr>) -> Self {
        DslExpr::Or(LogicalExpr { exprs })
    }

    /// Create a NOT logical expression.
    pub fn not(expr: DslExpr) -> Self {
        DslExpr::Not(UnaryExpr {
            expr: Box::new(expr),
        })
    }

    /// Create an IS NULL check.
    pub fn is_null(expr: DslExpr) -> Self {
        DslExpr::IsNull(UnaryExpr {
            expr: Box::new(expr),
        })
    }

    /// Create an IS NOT NULL check.
    pub fn is_not_null(expr: DslExpr) -> Self {
        DslExpr::IsNotNull(UnaryExpr {
            expr: Box::new(expr),
        })
    }

    /// Create a CASE expression.
    pub fn case(when: Vec<WhenClause>, else_expr: Option<DslExpr>) -> Self {
        DslExpr::Case(CaseExpr {
            when,
            else_expr: else_expr.map(Box::new),
        })
    }

    /// Create a COALESCE expression.
    pub fn coalesce(exprs: Vec<DslExpr>) -> Self {
        DslExpr::Coalesce(CoalesceExpr { exprs })
    }

    /// Create a DATE_TRUNC expression.
    pub fn date_trunc(grain: Grain, expr: DslExpr) -> Self {
        DslExpr::DateTrunc(DateTruncExpr {
            grain,
            expr: Box::new(expr),
        })
    }

    /// Create a guard expression (CASE WHEN condition THEN expr END).
    pub fn guard(condition: DslExpr, expr: DslExpr) -> Self {
        DslExpr::Guard(GuardExpr {
            condition: Box::new(condition),
            expr: Box::new(expr),
        })
    }
}

impl WhenClause {
    /// Create a new WHEN clause.
    pub fn new(condition: DslExpr, result: DslExpr) -> Self {
        WhenClause { condition, result }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_column_expr() {
        let expr = DslExpr::column("amount");
        match expr {
            DslExpr::Column(col) => assert_eq!(col.name, "amount"),
            _ => panic!("Expected Column variant"),
        }
    }

    #[test]
    fn test_literal_exprs() {
        let int_expr = DslExpr::int(42);
        match int_expr {
            DslExpr::Literal(LiteralExpr::Integer { value }) => assert_eq!(value, 42),
            _ => panic!("Expected Integer literal"),
        }

        let float_expr = DslExpr::float(3.14);
        match float_expr {
            DslExpr::Literal(LiteralExpr::Float { value }) => assert!((value - 3.14).abs() < 0.001),
            _ => panic!("Expected Float literal"),
        }

        let string_expr = DslExpr::string("hello");
        match string_expr {
            DslExpr::Literal(LiteralExpr::String { value }) => assert_eq!(value, "hello"),
            _ => panic!("Expected String literal"),
        }

        let bool_expr = DslExpr::bool(true);
        match bool_expr {
            DslExpr::Literal(LiteralExpr::Boolean { value }) => assert!(value),
            _ => panic!("Expected Boolean literal"),
        }

        let null_expr = DslExpr::null();
        match null_expr {
            DslExpr::Literal(LiteralExpr::Null) => (),
            _ => panic!("Expected Null literal"),
        }
    }

    #[test]
    fn test_aggregation_exprs() {
        let sum_expr = DslExpr::sum(DslExpr::column("amount"));
        match sum_expr {
            DslExpr::Sum(_) => (),
            _ => panic!("Expected Sum aggregation"),
        }

        let count_expr = DslExpr::count(DslExpr::column("id"));
        match count_expr {
            DslExpr::Count(_) => (),
            _ => panic!("Expected Count aggregation"),
        }

        let avg_expr = DslExpr::avg(DslExpr::column("price"));
        match avg_expr {
            DslExpr::Avg(_) => (),
            _ => panic!("Expected Avg aggregation"),
        }
    }

    #[test]
    fn test_binary_exprs() {
        let add_expr = DslExpr::add(DslExpr::column("a"), DslExpr::column("b"));
        match add_expr {
            DslExpr::Add(_) => (),
            _ => panic!("Expected Add expression"),
        }

        let eq_expr = DslExpr::eq(DslExpr::column("status"), DslExpr::string("active"));
        match eq_expr {
            DslExpr::Eq(_) => (),
            _ => panic!("Expected Eq expression"),
        }
    }

    #[test]
    fn test_logical_exprs() {
        let and_expr = DslExpr::and(vec![
            DslExpr::gt(DslExpr::column("amount"), DslExpr::int(100)),
            DslExpr::eq(DslExpr::column("status"), DslExpr::string("active")),
        ]);
        match and_expr {
            DslExpr::And(LogicalExpr { exprs }) => assert_eq!(exprs.len(), 2),
            _ => panic!("Expected And expression"),
        }

        let not_expr = DslExpr::not(DslExpr::is_null(DslExpr::column("name")));
        match not_expr {
            DslExpr::Not(_) => (),
            _ => panic!("Expected Not expression"),
        }
    }

    #[test]
    fn test_case_expr() {
        let case_expr = DslExpr::case(
            vec![
                WhenClause::new(
                    DslExpr::eq(DslExpr::column("status"), DslExpr::string("active")),
                    DslExpr::int(1),
                ),
                WhenClause::new(
                    DslExpr::eq(DslExpr::column("status"), DslExpr::string("inactive")),
                    DslExpr::int(0),
                ),
            ],
            Some(DslExpr::int(-1)),
        );

        match case_expr {
            DslExpr::Case(CaseExpr { when, else_expr }) => {
                assert_eq!(when.len(), 2);
                assert!(else_expr.is_some());
            }
            _ => panic!("Expected Case expression"),
        }
    }

    #[test]
    fn test_date_trunc_expr() {
        let expr = DslExpr::date_trunc(Grain::Month, DslExpr::column("order_date"));
        match expr {
            DslExpr::DateTrunc(DateTruncExpr { grain, .. }) => {
                assert_eq!(grain, Grain::Month);
            }
            _ => panic!("Expected DateTrunc expression"),
        }
    }

    #[test]
    fn test_guard_expr() {
        let guard_expr = DslExpr::guard(
            DslExpr::eq(DslExpr::column("category"), DslExpr::string("electronics")),
            DslExpr::column("amount"),
        );

        match guard_expr {
            DslExpr::Guard(_) => (),
            _ => panic!("Expected Guard expression"),
        }
    }

    #[test]
    fn test_serde_roundtrip() {
        let exprs = vec![
            DslExpr::column("amount"),
            DslExpr::int(42),
            DslExpr::sum(DslExpr::column("revenue")),
            DslExpr::add(DslExpr::column("a"), DslExpr::column("b")),
            DslExpr::eq(DslExpr::column("status"), DslExpr::string("active")),
            DslExpr::and(vec![
                DslExpr::gt(DslExpr::column("x"), DslExpr::int(0)),
                DslExpr::lt(DslExpr::column("y"), DslExpr::int(100)),
            ]),
        ];

        for expr in exprs {
            let json = serde_json::to_string(&expr).unwrap();
            let parsed: DslExpr = serde_json::from_str(&json).unwrap();
            assert_eq!(expr, parsed);
        }
    }

    #[test]
    fn test_nested_expr() {
        // (a + b) * c
        let expr = DslExpr::multiply(
            DslExpr::add(DslExpr::column("a"), DslExpr::column("b")),
            DslExpr::column("c"),
        );

        let json = serde_json::to_string(&expr).unwrap();
        let parsed: DslExpr = serde_json::from_str(&json).unwrap();
        assert_eq!(expr, parsed);
    }
}
