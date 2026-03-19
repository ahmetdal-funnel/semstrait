//! Unified expression type used across the entire pipeline.
//!
//! This is the single expression representation from YAML parsing through
//! planning, IR, SQL emission, and Substrait serialization.

use crate::grain::Grain;
use serde::{Deserialize, Serialize};

/// Unified expression tree for the semstrait pipeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Expr {
    /// Column reference by name, with optional qualifier (e.g., table.column).
    Column(ColumnRef),

    /// Literal value (integer, float, string, boolean, null).
    Literal(Literal),

    /// Reference to a semantic entity (measure, metric, dimension) by name.
    /// Resolved to `Column` during planning via column_mapping.
    EntityRef(EntityRef),

    /// Typed aggregation function.
    Aggregate(AggregateExpr),

    /// Binary operation (arithmetic, comparison, logical).
    BinaryOp(BinaryExpr),

    /// Unary negation (-expr).
    Negate(UnaryExpr),

    /// Logical NOT.
    Not(UnaryExpr),

    /// CASE WHEN ... THEN ... ELSE ... END.
    Case(CaseExpr),

    /// expr IN (list) or expr NOT IN (list).
    InList(InListExpr),

    /// expr BETWEEN low AND high.
    Between(BetweenExpr),

    /// expr LIKE pattern.
    Like(LikeExpr),

    /// expr IS NULL.
    IsNull(UnaryExpr),

    /// expr IS NOT NULL.
    IsNotNull(UnaryExpr),

    /// COALESCE(expr1, expr2, ...).
    Coalesce(CoalesceExpr),

    /// NULLIF(expr1, expr2).
    NullIf(NullIfExpr),

    /// DATE_TRUNC(grain, expr).
    DateTrunc(DateTruncExpr),

    /// Generic function call (escape hatch for non-standard functions).
    FunctionCall(FunctionCallExpr),

    /// Guard: CASE WHEN condition THEN expr ELSE NULL END.
    /// Sugar for measure filters; resolved during planning.
    Guard(GuardExpr),
}

// ─── Leaf types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnRef {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qualifier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Literal {
    Integer { value: i64 },
    Float { value: f64 },
    String { value: String },
    Boolean { value: bool },
    Null,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityRef {
    pub name: String,
}

// ─── Aggregation ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggregateExpr {
    pub function: Aggregation,
    pub expr: Box<Expr>,
    pub distinct: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Aggregation {
    Sum,
    Avg,
    Count,
    CountDistinct,
    Min,
    Max,
}

impl Aggregation {
    /// SQL function name for this aggregation.
    pub fn sql_name(&self) -> &'static str {
        match self {
            Aggregation::Sum => "SUM",
            Aggregation::Avg => "AVG",
            Aggregation::Count | Aggregation::CountDistinct => "COUNT",
            Aggregation::Min => "MIN",
            Aggregation::Max => "MAX",
        }
    }
}

// ─── Binary / Unary ──────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BinaryExpr {
    pub left: Box<Expr>,
    pub op: BinaryOp,
    pub right: Box<Expr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BinaryOp {
    // Arithmetic
    Add,
    Subtract,
    Multiply,
    Divide,
    /// Safe division: returns NULL when divisor is zero.
    SafeDivide,
    // Comparison
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    // Logical
    And,
    Or,
}

impl BinaryOp {
    pub fn as_str(&self) -> &'static str {
        match self {
            BinaryOp::Add => "+",
            BinaryOp::Subtract => "-",
            BinaryOp::Multiply => "*",
            BinaryOp::Divide => "/",
            BinaryOp::SafeDivide => "/",
            BinaryOp::Eq => "=",
            BinaryOp::NotEq => "!=",
            BinaryOp::Lt => "<",
            BinaryOp::LtEq => "<=",
            BinaryOp::Gt => ">",
            BinaryOp::GtEq => ">=",
            BinaryOp::And => "AND",
            BinaryOp::Or => "OR",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnaryExpr {
    pub expr: Box<Expr>,
}

// ─── Compound expressions ────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaseExpr {
    pub when_then: Vec<WhenClause>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub else_expr: Option<Box<Expr>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WhenClause {
    pub condition: Expr,
    pub result: Expr,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InListExpr {
    pub expr: Box<Expr>,
    pub list: Vec<Expr>,
    pub negated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetweenExpr {
    pub expr: Box<Expr>,
    pub low: Box<Expr>,
    pub high: Box<Expr>,
    pub negated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LikeExpr {
    pub expr: Box<Expr>,
    pub pattern: Box<Expr>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoalesceExpr {
    pub exprs: Vec<Expr>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NullIfExpr {
    pub expr: Box<Expr>,
    pub null_expr: Box<Expr>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DateTruncExpr {
    pub grain: Grain,
    pub expr: Box<Expr>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionCallExpr {
    pub name: String,
    pub args: Vec<Expr>,
    pub distinct: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuardExpr {
    pub condition: Box<Expr>,
    pub expr: Box<Expr>,
}

// ─── Convenience constructors ────────────────────────────────────────

#[allow(clippy::should_implement_trait)]
impl Expr {
    pub fn column(name: impl Into<String>) -> Self {
        Expr::Column(ColumnRef {
            name: name.into(),
            qualifier: None,
        })
    }

    pub fn qualified_column(qualifier: impl Into<String>, name: impl Into<String>) -> Self {
        Expr::Column(ColumnRef {
            name: name.into(),
            qualifier: Some(qualifier.into()),
        })
    }

    pub fn entity_ref(name: impl Into<String>) -> Self {
        Expr::EntityRef(EntityRef { name: name.into() })
    }

    pub fn int(value: i64) -> Self {
        Expr::Literal(Literal::Integer { value })
    }

    pub fn float(value: f64) -> Self {
        Expr::Literal(Literal::Float { value })
    }

    pub fn string(value: impl Into<String>) -> Self {
        Expr::Literal(Literal::String {
            value: value.into(),
        })
    }

    pub fn boolean(value: bool) -> Self {
        Expr::Literal(Literal::Boolean { value })
    }

    pub fn null() -> Self {
        Expr::Literal(Literal::Null)
    }

    // ── Aggregation constructors ─────────────────────────────────────

    pub fn sum(expr: Expr) -> Self {
        Expr::Aggregate(AggregateExpr {
            function: Aggregation::Sum,
            expr: Box::new(expr),
            distinct: false,
        })
    }

    pub fn avg(expr: Expr) -> Self {
        Expr::Aggregate(AggregateExpr {
            function: Aggregation::Avg,
            expr: Box::new(expr),
            distinct: false,
        })
    }

    pub fn count(expr: Expr) -> Self {
        Expr::Aggregate(AggregateExpr {
            function: Aggregation::Count,
            expr: Box::new(expr),
            distinct: false,
        })
    }

    pub fn count_distinct(expr: Expr) -> Self {
        Expr::Aggregate(AggregateExpr {
            function: Aggregation::CountDistinct,
            expr: Box::new(expr),
            distinct: true,
        })
    }

    pub fn min(expr: Expr) -> Self {
        Expr::Aggregate(AggregateExpr {
            function: Aggregation::Min,
            expr: Box::new(expr),
            distinct: false,
        })
    }

    pub fn max(expr: Expr) -> Self {
        Expr::Aggregate(AggregateExpr {
            function: Aggregation::Max,
            expr: Box::new(expr),
            distinct: false,
        })
    }

    // ── Binary operation constructors ────────────────────────────────

    pub fn binary(left: Expr, op: BinaryOp, right: Expr) -> Self {
        Expr::BinaryOp(BinaryExpr {
            left: Box::new(left),
            op,
            right: Box::new(right),
        })
    }

    pub fn add(left: Expr, right: Expr) -> Self {
        Self::binary(left, BinaryOp::Add, right)
    }

    pub fn subtract(left: Expr, right: Expr) -> Self {
        Self::binary(left, BinaryOp::Subtract, right)
    }

    pub fn multiply(left: Expr, right: Expr) -> Self {
        Self::binary(left, BinaryOp::Multiply, right)
    }

    pub fn divide(left: Expr, right: Expr) -> Self {
        Self::binary(left, BinaryOp::Divide, right)
    }

    pub fn safe_divide(left: Expr, right: Expr) -> Self {
        Self::binary(left, BinaryOp::SafeDivide, right)
    }

    pub fn eq(left: Expr, right: Expr) -> Self {
        Self::binary(left, BinaryOp::Eq, right)
    }

    pub fn ne(left: Expr, right: Expr) -> Self {
        Self::binary(left, BinaryOp::NotEq, right)
    }

    pub fn gt(left: Expr, right: Expr) -> Self {
        Self::binary(left, BinaryOp::Gt, right)
    }

    pub fn gte(left: Expr, right: Expr) -> Self {
        Self::binary(left, BinaryOp::GtEq, right)
    }

    pub fn lt(left: Expr, right: Expr) -> Self {
        Self::binary(left, BinaryOp::Lt, right)
    }

    pub fn lte(left: Expr, right: Expr) -> Self {
        Self::binary(left, BinaryOp::LtEq, right)
    }

    // ── Logical constructors ─────────────────────────────────────────

    pub fn and(left: Expr, right: Expr) -> Self {
        Self::binary(left, BinaryOp::And, right)
    }

    pub fn and_many(exprs: Vec<Expr>) -> Option<Self> {
        exprs.into_iter().reduce(Self::and)
    }

    pub fn or(left: Expr, right: Expr) -> Self {
        Self::binary(left, BinaryOp::Or, right)
    }

    pub fn or_many(exprs: Vec<Expr>) -> Option<Self> {
        exprs.into_iter().reduce(Self::or)
    }

    pub fn not(expr: Expr) -> Self {
        Expr::Not(UnaryExpr {
            expr: Box::new(expr),
        })
    }

    pub fn negate(expr: Expr) -> Self {
        Expr::Negate(UnaryExpr {
            expr: Box::new(expr),
        })
    }

    // ── Predicate constructors ───────────────────────────────────────

    pub fn is_null(expr: Expr) -> Self {
        Expr::IsNull(UnaryExpr {
            expr: Box::new(expr),
        })
    }

    pub fn is_not_null(expr: Expr) -> Self {
        Expr::IsNotNull(UnaryExpr {
            expr: Box::new(expr),
        })
    }

    pub fn in_list(expr: Expr, list: Vec<Expr>) -> Self {
        Expr::InList(InListExpr {
            expr: Box::new(expr),
            list,
            negated: false,
        })
    }

    pub fn not_in_list(expr: Expr, list: Vec<Expr>) -> Self {
        Expr::InList(InListExpr {
            expr: Box::new(expr),
            list,
            negated: true,
        })
    }

    pub fn between(expr: Expr, low: Expr, high: Expr) -> Self {
        Expr::Between(BetweenExpr {
            expr: Box::new(expr),
            low: Box::new(low),
            high: Box::new(high),
            negated: false,
        })
    }

    pub fn like(expr: Expr, pattern: Expr) -> Self {
        Expr::Like(LikeExpr {
            expr: Box::new(expr),
            pattern: Box::new(pattern),
        })
    }

    // ── Conditional constructors ─────────────────────────────────────

    pub fn case(when_then: Vec<WhenClause>, else_expr: Option<Expr>) -> Self {
        Expr::Case(CaseExpr {
            when_then,
            else_expr: else_expr.map(Box::new),
        })
    }

    pub fn coalesce(exprs: Vec<Expr>) -> Self {
        Expr::Coalesce(CoalesceExpr { exprs })
    }

    pub fn null_if(expr: Expr, null_expr: Expr) -> Self {
        Expr::NullIf(NullIfExpr {
            expr: Box::new(expr),
            null_expr: Box::new(null_expr),
        })
    }

    pub fn date_trunc(grain: Grain, expr: Expr) -> Self {
        Expr::DateTrunc(DateTruncExpr {
            grain,
            expr: Box::new(expr),
        })
    }

    pub fn function_call(name: impl Into<String>, args: Vec<Expr>) -> Self {
        Expr::FunctionCall(FunctionCallExpr {
            name: name.into(),
            args,
            distinct: false,
        })
    }

    pub fn guard(condition: Expr, expr: Expr) -> Self {
        Expr::Guard(GuardExpr {
            condition: Box::new(condition),
            expr: Box::new(expr),
        })
    }

    // ── Tree transformation ─────────────────────────────────────────

    /// Bottom-up tree transformation.
    ///
    /// Recursively transforms all children first, rebuilds the node,
    /// then applies `f`. If `f` returns `Ok(Some(new))`, that replaces
    /// the node. If `f` returns `Ok(None)`, the recursively-rebuilt
    /// node is kept as-is.
    pub fn transform<F, E>(&self, f: &F) -> Result<Expr, E>
    where
        F: Fn(&Expr) -> Result<Option<Expr>, E>,
    {
        // Step 1: recursively transform children.
        let recursed = self.transform_children(f)?;
        // Step 2: apply the closure to the rebuilt node.
        match f(&recursed)? {
            Some(replaced) => Ok(replaced),
            None => Ok(recursed),
        }
    }

    /// Rebuild this node with all children transformed (but don't transform self).
    fn transform_children<F, E>(&self, f: &F) -> Result<Expr, E>
    where
        F: Fn(&Expr) -> Result<Option<Expr>, E>,
    {
        match self {
            // Leaf nodes — no children to recurse into.
            Expr::Column(_) | Expr::Literal(_) | Expr::EntityRef(_) => Ok(self.clone()),

            Expr::Aggregate(agg) => Ok(Expr::Aggregate(AggregateExpr {
                function: agg.function,
                expr: Box::new(agg.expr.transform(f)?),
                distinct: agg.distinct,
            })),

            Expr::BinaryOp(bin) => Ok(Expr::binary(
                bin.left.transform(f)?,
                bin.op,
                bin.right.transform(f)?,
            )),

            Expr::Negate(u) => Ok(Expr::negate(u.expr.transform(f)?)),
            Expr::Not(u) => Ok(Expr::not(u.expr.transform(f)?)),
            Expr::IsNull(u) => Ok(Expr::is_null(u.expr.transform(f)?)),
            Expr::IsNotNull(u) => Ok(Expr::is_not_null(u.expr.transform(f)?)),

            Expr::InList(il) => {
                let expr = il.expr.transform(f)?;
                let list = il
                    .list
                    .iter()
                    .map(|e| e.transform(f))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Expr::InList(InListExpr {
                    expr: Box::new(expr),
                    list,
                    negated: il.negated,
                }))
            }

            Expr::Between(bt) => Ok(Expr::Between(BetweenExpr {
                expr: Box::new(bt.expr.transform(f)?),
                low: Box::new(bt.low.transform(f)?),
                high: Box::new(bt.high.transform(f)?),
                negated: bt.negated,
            })),

            Expr::Like(lk) => Ok(Expr::Like(LikeExpr {
                expr: Box::new(lk.expr.transform(f)?),
                pattern: Box::new(lk.pattern.transform(f)?),
            })),

            Expr::Case(c) => {
                let when_then = c
                    .when_then
                    .iter()
                    .map(|wc| {
                        Ok(WhenClause::new(
                            wc.condition.transform(f)?,
                            wc.result.transform(f)?,
                        ))
                    })
                    .collect::<Result<Vec<_>, E>>()?;
                let else_expr = c
                    .else_expr
                    .as_ref()
                    .map(|e| e.transform(f))
                    .transpose()?;
                Ok(Expr::case(when_then, else_expr))
            }

            Expr::Coalesce(co) => {
                let exprs = co
                    .exprs
                    .iter()
                    .map(|e| e.transform(f))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Expr::coalesce(exprs))
            }

            Expr::NullIf(ni) => Ok(Expr::null_if(
                ni.expr.transform(f)?,
                ni.null_expr.transform(f)?,
            )),

            Expr::DateTrunc(dt) => {
                Ok(Expr::date_trunc(dt.grain, dt.expr.transform(f)?))
            }

            Expr::FunctionCall(fc) => {
                let args = fc
                    .args
                    .iter()
                    .map(|a| a.transform(f))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Expr::FunctionCall(FunctionCallExpr {
                    name: fc.name.clone(),
                    args,
                    distinct: fc.distinct,
                }))
            }

            Expr::Guard(g) => Ok(Expr::guard(
                g.condition.transform(f)?,
                g.expr.transform(f)?,
            )),
        }
    }
}

impl WhenClause {
    pub fn new(condition: Expr, result: Expr) -> Self {
        WhenClause { condition, result }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_column_expr() {
        let expr = Expr::column("amount");
        match &expr {
            Expr::Column(col) => {
                assert_eq!(col.name, "amount");
                assert_eq!(col.qualifier, None);
            }
            _ => panic!("Expected Column"),
        }
    }

    #[test]
    fn test_qualified_column() {
        let expr = Expr::qualified_column("orders", "amount");
        match &expr {
            Expr::Column(col) => {
                assert_eq!(col.name, "amount");
                assert_eq!(col.qualifier, Some("orders".to_string()));
            }
            _ => panic!("Expected Column"),
        }
    }

    #[test]
    fn test_literals_preserve_types() {
        assert_eq!(Expr::int(42), Expr::Literal(Literal::Integer { value: 42 }));
        assert_eq!(
            Expr::float(3.14),
            Expr::Literal(Literal::Float { value: 3.14 })
        );
        assert_eq!(
            Expr::string("hello"),
            Expr::Literal(Literal::String {
                value: "hello".to_string()
            })
        );
        assert_eq!(
            Expr::boolean(true),
            Expr::Literal(Literal::Boolean { value: true })
        );
        assert_eq!(Expr::null(), Expr::Literal(Literal::Null));
    }

    #[test]
    fn test_integer_precision() {
        let big = i64::MAX;
        let expr = Expr::int(big);
        match &expr {
            Expr::Literal(Literal::Integer { value }) => assert_eq!(*value, big),
            _ => panic!("Expected Integer literal"),
        }
    }

    #[test]
    fn test_aggregate_constructors() {
        let sum = Expr::sum(Expr::column("amount"));
        match &sum {
            Expr::Aggregate(agg) => {
                assert_eq!(agg.function, Aggregation::Sum);
                assert!(!agg.distinct);
            }
            _ => panic!("Expected Aggregate"),
        }

        let cd = Expr::count_distinct(Expr::column("id"));
        match &cd {
            Expr::Aggregate(agg) => {
                assert_eq!(agg.function, Aggregation::CountDistinct);
                assert!(agg.distinct);
            }
            _ => panic!("Expected Aggregate"),
        }
    }

    #[test]
    fn test_binary_ops() {
        let add = Expr::add(Expr::column("a"), Expr::column("b"));
        match &add {
            Expr::BinaryOp(bin) => assert_eq!(bin.op, BinaryOp::Add),
            _ => panic!("Expected BinaryOp"),
        }

        let eq = Expr::eq(Expr::column("status"), Expr::string("active"));
        match &eq {
            Expr::BinaryOp(bin) => assert_eq!(bin.op, BinaryOp::Eq),
            _ => panic!("Expected BinaryOp"),
        }
    }

    #[test]
    fn test_logical_and_many() {
        let and = Expr::and_many(vec![
            Expr::gt(Expr::column("x"), Expr::int(0)),
            Expr::lt(Expr::column("y"), Expr::int(100)),
            Expr::eq(Expr::column("z"), Expr::string("active")),
        ])
        .expect("non-empty vec");
        // Should be nested: ((x > 0) AND (y < 100)) AND (z = 'active')
        match &and {
            Expr::BinaryOp(bin) => assert_eq!(bin.op, BinaryOp::And),
            _ => panic!("Expected BinaryOp And"),
        }
    }

    #[test]
    fn test_case_expr() {
        let case = Expr::case(
            vec![
                WhenClause::new(
                    Expr::eq(Expr::column("status"), Expr::string("active")),
                    Expr::int(1),
                ),
                WhenClause::new(
                    Expr::eq(Expr::column("status"), Expr::string("inactive")),
                    Expr::int(0),
                ),
            ],
            Some(Expr::int(-1)),
        );

        match &case {
            Expr::Case(c) => {
                assert_eq!(c.when_then.len(), 2);
                assert!(c.else_expr.is_some());
            }
            _ => panic!("Expected Case"),
        }
    }

    #[test]
    fn test_guard_expr() {
        let guard = Expr::guard(
            Expr::eq(Expr::column("category"), Expr::string("electronics")),
            Expr::column("amount"),
        );
        assert!(matches!(guard, Expr::Guard(_)));
    }

    #[test]
    fn test_date_trunc() {
        let expr = Expr::date_trunc(Grain::Month, Expr::column("order_date"));
        match &expr {
            Expr::DateTrunc(dt) => assert_eq!(dt.grain, Grain::Month),
            _ => panic!("Expected DateTrunc"),
        }
    }

    #[test]
    fn test_serde_roundtrip() {
        let exprs = vec![
            Expr::column("amount"),
            Expr::int(42),
            Expr::float(3.14),
            Expr::sum(Expr::column("revenue")),
            Expr::add(Expr::column("a"), Expr::column("b")),
            Expr::eq(Expr::column("status"), Expr::string("active")),
            Expr::and(
                Expr::gt(Expr::column("x"), Expr::int(0)),
                Expr::lt(Expr::column("y"), Expr::int(100)),
            ),
            Expr::guard(Expr::boolean(true), Expr::column("val")),
        ];

        for expr in exprs {
            let json = serde_json::to_string(&expr).unwrap();
            let parsed: Expr = serde_json::from_str(&json).unwrap();
            assert_eq!(expr, parsed);
        }
    }

    #[test]
    fn test_nested_expr() {
        // (a + b) * c
        let expr = Expr::multiply(
            Expr::add(Expr::column("a"), Expr::column("b")),
            Expr::column("c"),
        );

        let json = serde_json::to_string(&expr).unwrap();
        let parsed: Expr = serde_json::from_str(&json).unwrap();
        assert_eq!(expr, parsed);
    }

    #[test]
    fn test_binary_op_as_str() {
        assert_eq!(BinaryOp::Add.as_str(), "+");
        assert_eq!(BinaryOp::Eq.as_str(), "=");
        assert_eq!(BinaryOp::And.as_str(), "AND");
        assert_eq!(BinaryOp::SafeDivide.as_str(), "/");
    }

    #[test]
    fn test_transform_identity() {
        let expr = Expr::add(Expr::column("a"), Expr::int(1));
        let result: Result<Expr, std::convert::Infallible> =
            expr.transform(&|_| Ok(None));
        assert_eq!(result.unwrap(), expr);
    }

    #[test]
    fn test_transform_rename_columns() {
        // Rename column "a" → "x" bottom-up
        let expr = Expr::add(
            Expr::sum(Expr::column("a")),
            Expr::column("b"),
        );
        let result: Result<Expr, std::convert::Infallible> = expr.transform(&|e| {
            if let Expr::Column(col) = e {
                if col.name == "a" {
                    return Ok(Some(Expr::column("x")));
                }
            }
            Ok(None)
        });
        let expected = Expr::add(
            Expr::sum(Expr::column("x")),
            Expr::column("b"),
        );
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn test_transform_expand_guard() {
        let expr = Expr::guard(Expr::boolean(true), Expr::column("val"));
        let result: Result<Expr, std::convert::Infallible> = expr.transform(&|e| {
            if let Expr::Guard(g) = e {
                return Ok(Some(Expr::case(
                    vec![WhenClause::new((*g.condition).clone(), (*g.expr).clone())],
                    Some(Expr::null()),
                )));
            }
            Ok(None)
        });
        match result.unwrap() {
            Expr::Case(c) => {
                assert_eq!(c.when_then.len(), 1);
                assert_eq!(c.else_expr, Some(Box::new(Expr::null())));
            }
            other => panic!("Expected Case, got {:?}", other),
        }
    }
}
