//! DSL expression AST.
//!
//! This AST is distinct from the planner's `Expr` type. It represents the
//! surface-level DSL syntax before semantic resolution. The `lower` module
//! converts `DslExpr` → planner `Expr` once names are resolved.

/// A parsed DSL expression.
#[derive(Debug, Clone, PartialEq)]
pub enum DslExpr {
    /// A numeric literal: `42`, `3.14`.
    Number(f64),
    /// A string literal: `'hello'`.
    StringLit(String),
    /// Boolean literal: `TRUE`, `FALSE`.
    Bool(bool),
    /// NULL literal.
    Null,
    /// A column or name reference: `amount`, `orders.amount`.
    ColumnRef(ColumnRef),
    /// A function call: `SUM(amount)`, `COUNT(DISTINCT id)`.
    FunctionCall(FunctionCall),
    /// Binary arithmetic/comparison: `a + b`, `a > b`.
    BinaryOp {
        left: Box<DslExpr>,
        op: BinaryOp,
        right: Box<DslExpr>,
    },
    /// Unary negation: `-expr`.
    Negate(Box<DslExpr>),
    /// CASE WHEN ... THEN ... [ELSE ...] END.
    Case(CaseExpr),
    /// Parenthesized expression (preserved for clarity in lowering).
    Paren(Box<DslExpr>),
}

/// A column or measure/metric name reference.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnRef {
    /// Optional qualifier: `orders` in `orders.amount`.
    pub qualifier: Option<String>,
    /// The name: `amount`.
    pub name: String,
}

/// A function call with optional DISTINCT modifier.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionCall {
    pub name: String,
    pub distinct: bool,
    pub args: Vec<DslExpr>,
}

/// Binary operators supported in DSL expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    // Arithmetic
    Add,
    Subtract,
    Multiply,
    Divide,
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
    /// Binding power for Pratt parsing (left, right).
    pub fn binding_power(self) -> (u8, u8) {
        match self {
            Self::Or => (1, 2),
            Self::And => (3, 4),
            Self::Eq | Self::NotEq | Self::Lt | Self::LtEq | Self::Gt | Self::GtEq => (5, 6),
            Self::Add | Self::Subtract => (7, 8),
            Self::Multiply | Self::Divide => (9, 10),
        }
    }
}

/// A CASE expression.
#[derive(Debug, Clone, PartialEq)]
pub struct CaseExpr {
    pub when_clauses: Vec<WhenClause>,
    pub else_expr: Option<Box<DslExpr>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WhenClause {
    pub condition: DslExpr,
    pub result: DslExpr,
}
