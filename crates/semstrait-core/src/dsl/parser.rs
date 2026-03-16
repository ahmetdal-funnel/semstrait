//! Recursive-descent Pratt parser for DSL expressions.
//!
//! Grammar:
//! ```text
//! expr         = pratt_expr(min_bp=0)
//! pratt_expr   = prefix (infix)*
//! prefix       = function_call | column_ref | literal | '(' expr ')' | case_expr | '-' prefix
//! infix        = ('+' | '-' | '*' | '/' | '=' | '!=' | '<' | '<=' | '>' | '>=' | AND | OR) pratt_expr
//! function_call = IDENT '(' [DISTINCT] args ')'
//! column_ref   = IDENT ['.' IDENT]
//! args         = expr (',' expr)*
//! case_expr    = CASE when_clause+ [ELSE expr] END
//! when_clause  = WHEN expr THEN expr
//! ```

use super::ast::*;
use super::lexer::SpannedToken;
use super::token::Token;

/// Parse error with position information.
#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub position: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "parse error at {}: {}", self.position, self.message)
    }
}

impl std::error::Error for ParseError {}

/// Parser state.
struct Parser {
    tokens: Vec<SpannedToken>,
    pos: usize,
}

/// Parse a DSL expression string into an AST.
pub fn parse_dsl(input: &str) -> Result<DslExpr, ParseError> {
    let tokens = super::lexer::tokenize(input).map_err(|e| ParseError {
        message: e.to_string(),
        position: e.span.start,
    })?;

    if tokens.is_empty() {
        return Err(ParseError {
            message: "empty expression".to_string(),
            position: 0,
        });
    }

    let mut parser = Parser { tokens, pos: 0 };
    let expr = parser.parse_expr(0)?;

    if parser.pos < parser.tokens.len() {
        let span = &parser.tokens[parser.pos].span;
        return Err(ParseError {
            message: format!(
                "unexpected token {:?}",
                parser.tokens[parser.pos].token
            ),
            position: span.start,
        });
    }

    Ok(expr)
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos).map(|t| &t.token)
    }

    fn current_position(&self) -> usize {
        self.tokens
            .get(self.pos)
            .map(|t| t.span.start)
            .unwrap_or(0)
    }

    fn advance(&mut self) -> &SpannedToken {
        let t = &self.tokens[self.pos];
        self.pos += 1;
        t
    }

    fn expect_token(&mut self, expected: &Token) -> Result<(), ParseError> {
        match self.peek() {
            Some(t) if t == expected => {
                self.advance();
                Ok(())
            }
            Some(t) => Err(ParseError {
                message: format!("expected {:?}, found {:?}", expected, t),
                position: self.current_position(),
            }),
            None => Err(ParseError {
                message: format!("expected {:?}, found end of input", expected),
                position: self.current_position(),
            }),
        }
    }

    fn expect_keyword(&mut self, kw: &str) -> Result<(), ParseError> {
        match self.peek() {
            Some(Token::Ident(s)) if s.eq_ignore_ascii_case(kw) => {
                self.advance();
                Ok(())
            }
            other => Err(ParseError {
                message: format!("expected keyword '{}', found {:?}", kw, other),
                position: self.current_position(),
            }),
        }
    }

    fn is_keyword(&self, kw: &str) -> bool {
        matches!(self.peek(), Some(Token::Ident(s)) if s.eq_ignore_ascii_case(kw))
    }

    /// Pratt parser entry point.
    fn parse_expr(&mut self, min_bp: u8) -> Result<DslExpr, ParseError> {
        let mut lhs = self.parse_prefix()?;

        loop {
            let Some(op) = self.peek_binary_op() else {
                break;
            };
            let (l_bp, r_bp) = op.binding_power();
            if l_bp < min_bp {
                break;
            }
            self.advance(); // consume the operator token
            let rhs = self.parse_expr(r_bp)?;
            lhs = DslExpr::BinaryOp {
                left: Box::new(lhs),
                op,
                right: Box::new(rhs),
            };
        }

        Ok(lhs)
    }

    fn peek_binary_op(&self) -> Option<BinaryOp> {
        match self.peek()? {
            Token::Plus => Some(BinaryOp::Add),
            Token::Minus => Some(BinaryOp::Subtract),
            Token::Star => Some(BinaryOp::Multiply),
            Token::Slash => Some(BinaryOp::Divide),
            Token::Eq => Some(BinaryOp::Eq),
            Token::NotEq | Token::LtGt => Some(BinaryOp::NotEq),
            Token::Lt => Some(BinaryOp::Lt),
            Token::LtEq => Some(BinaryOp::LtEq),
            Token::Gt => Some(BinaryOp::Gt),
            Token::GtEq => Some(BinaryOp::GtEq),
            Token::Ident(s) if s.eq_ignore_ascii_case("AND") => Some(BinaryOp::And),
            Token::Ident(s) if s.eq_ignore_ascii_case("OR") => Some(BinaryOp::Or),
            _ => None,
        }
    }

    fn parse_prefix(&mut self) -> Result<DslExpr, ParseError> {
        match self.peek() {
            Some(Token::Number(_)) => {
                let Token::Number(n) = self.advance().token.clone() else {
                    unreachable!()
                };
                Ok(DslExpr::Number(n))
            }
            Some(Token::StringLit(_)) => {
                let Token::StringLit(s) = self.advance().token.clone() else {
                    unreachable!()
                };
                Ok(DslExpr::StringLit(s))
            }
            Some(Token::Minus) => {
                self.advance();
                let expr = self.parse_prefix()?;
                Ok(DslExpr::Negate(Box::new(expr)))
            }
            Some(Token::LParen) => {
                self.advance();
                let expr = self.parse_expr(0)?;
                self.expect_token(&Token::RParen)?;
                Ok(DslExpr::Paren(Box::new(expr)))
            }
            Some(Token::Ident(s)) if s.eq_ignore_ascii_case("CASE") => self.parse_case(),
            Some(Token::Ident(s)) if s.eq_ignore_ascii_case("TRUE") => {
                self.advance();
                Ok(DslExpr::Bool(true))
            }
            Some(Token::Ident(s)) if s.eq_ignore_ascii_case("FALSE") => {
                self.advance();
                Ok(DslExpr::Bool(false))
            }
            Some(Token::Ident(s)) if s.eq_ignore_ascii_case("NULL") => {
                self.advance();
                Ok(DslExpr::Null)
            }
            Some(Token::Ident(_)) => self.parse_ident_expr(),
            _ => Err(ParseError {
                message: format!("unexpected token {:?}", self.peek()),
                position: self.current_position(),
            }),
        }
    }

    /// Parse an identifier-starting expression: could be a function call or column ref.
    fn parse_ident_expr(&mut self) -> Result<DslExpr, ParseError> {
        let Token::Ident(name) = self.advance().token.clone() else {
            unreachable!()
        };

        // Check for function call: IDENT '('
        if matches!(self.peek(), Some(Token::LParen)) {
            return self.parse_function_call(name);
        }

        // Check for qualified name: IDENT '.' IDENT
        if matches!(self.peek(), Some(Token::Dot)) {
            self.advance(); // consume '.'
            match self.peek() {
                Some(Token::Ident(_)) => {
                    let Token::Ident(attr) = self.advance().token.clone() else {
                        unreachable!()
                    };
                    return Ok(DslExpr::ColumnRef(ColumnRef {
                        qualifier: Some(name),
                        name: attr,
                    }));
                }
                _ => {
                    return Err(ParseError {
                        message: "expected identifier after '.'".to_string(),
                        position: self.current_position(),
                    });
                }
            }
        }

        // Simple name reference
        Ok(DslExpr::ColumnRef(ColumnRef {
            qualifier: None,
            name,
        }))
    }

    fn parse_function_call(&mut self, name: String) -> Result<DslExpr, ParseError> {
        self.expect_token(&Token::LParen)?;

        // Check for empty args
        if matches!(self.peek(), Some(Token::RParen)) {
            self.advance();
            return Ok(DslExpr::FunctionCall(FunctionCall {
                name,
                distinct: false,
                args: vec![],
            }));
        }

        // Check for DISTINCT modifier
        let distinct = if self.is_keyword("DISTINCT") {
            self.advance();
            true
        } else {
            false
        };

        let mut args = vec![self.parse_expr(0)?];
        while matches!(self.peek(), Some(Token::Comma)) {
            self.advance();
            args.push(self.parse_expr(0)?);
        }

        self.expect_token(&Token::RParen)?;

        Ok(DslExpr::FunctionCall(FunctionCall {
            name,
            distinct,
            args,
        }))
    }

    fn parse_case(&mut self) -> Result<DslExpr, ParseError> {
        self.expect_keyword("CASE")?;

        let mut when_clauses = Vec::new();
        while self.is_keyword("WHEN") {
            self.advance(); // consume WHEN
            let condition = self.parse_expr(0)?;
            self.expect_keyword("THEN")?;
            let result = self.parse_expr(0)?;
            when_clauses.push(WhenClause { condition, result });
        }

        if when_clauses.is_empty() {
            return Err(ParseError {
                message: "CASE requires at least one WHEN clause".to_string(),
                position: self.current_position(),
            });
        }

        let else_expr = if self.is_keyword("ELSE") {
            self.advance();
            Some(Box::new(self.parse_expr(0)?))
        } else {
            None
        };

        self.expect_keyword("END")?;

        Ok(DslExpr::Case(CaseExpr {
            when_clauses,
            else_expr,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_column_ref() {
        let expr = parse_dsl("amount").unwrap();
        assert_eq!(
            expr,
            DslExpr::ColumnRef(ColumnRef {
                qualifier: None,
                name: "amount".to_string(),
            })
        );
    }

    #[test]
    fn test_qualified_column_ref() {
        let expr = parse_dsl("orders.amount").unwrap();
        assert_eq!(
            expr,
            DslExpr::ColumnRef(ColumnRef {
                qualifier: Some("orders".to_string()),
                name: "amount".to_string(),
            })
        );
    }

    #[test]
    fn test_sum_function() {
        let expr = parse_dsl("SUM(amount)").unwrap();
        assert_eq!(
            expr,
            DslExpr::FunctionCall(FunctionCall {
                name: "SUM".to_string(),
                distinct: false,
                args: vec![DslExpr::ColumnRef(ColumnRef {
                    qualifier: None,
                    name: "amount".to_string(),
                })],
            })
        );
    }

    #[test]
    fn test_count_distinct() {
        let expr = parse_dsl("COUNT(DISTINCT user_id)").unwrap();
        assert_eq!(
            expr,
            DslExpr::FunctionCall(FunctionCall {
                name: "COUNT".to_string(),
                distinct: true,
                args: vec![DslExpr::ColumnRef(ColumnRef {
                    qualifier: None,
                    name: "user_id".to_string(),
                })],
            })
        );
    }

    #[test]
    fn test_arithmetic() {
        let expr = parse_dsl("revenue / users").unwrap();
        match expr {
            DslExpr::BinaryOp { op, .. } => assert_eq!(op, BinaryOp::Divide),
            _ => panic!("expected BinaryOp"),
        }
    }

    #[test]
    fn test_complex_arithmetic() {
        // a + b * c should parse as a + (b * c) due to precedence
        let expr = parse_dsl("a + b * c").unwrap();
        match &expr {
            DslExpr::BinaryOp { op: BinaryOp::Add, right, .. } => {
                assert!(matches!(right.as_ref(), DslExpr::BinaryOp { op: BinaryOp::Multiply, .. }));
            }
            _ => panic!("expected Add at top level"),
        }
    }

    #[test]
    fn test_parenthesized() {
        let expr = parse_dsl("(a + b) * c").unwrap();
        match &expr {
            DslExpr::BinaryOp { op: BinaryOp::Multiply, left, .. } => {
                assert!(matches!(left.as_ref(), DslExpr::Paren(_)));
            }
            _ => panic!("expected Multiply at top level"),
        }
    }

    #[test]
    fn test_case_expression() {
        let expr = parse_dsl("CASE WHEN status != 'cancelled' THEN amount ELSE 0 END").unwrap();
        match expr {
            DslExpr::Case(case) => {
                assert_eq!(case.when_clauses.len(), 1);
                assert!(case.else_expr.is_some());
            }
            _ => panic!("expected Case"),
        }
    }

    #[test]
    fn test_nested_function() {
        let expr = parse_dsl("SUM(CASE WHEN status != 'cancelled' THEN amount END)").unwrap();
        match &expr {
            DslExpr::FunctionCall(fc) => {
                assert_eq!(fc.name, "SUM");
                assert_eq!(fc.args.len(), 1);
                assert!(matches!(&fc.args[0], DslExpr::Case(_)));
            }
            _ => panic!("expected FunctionCall"),
        }
    }

    #[test]
    fn test_multi_arg_function() {
        let expr = parse_dsl("COALESCE(a, b, 0)").unwrap();
        match &expr {
            DslExpr::FunctionCall(fc) => {
                assert_eq!(fc.name, "COALESCE");
                assert_eq!(fc.args.len(), 3);
            }
            _ => panic!("expected FunctionCall"),
        }
    }

    #[test]
    fn test_number_literal() {
        let expr = parse_dsl("42").unwrap();
        assert_eq!(expr, DslExpr::Number(42.0));
    }

    #[test]
    fn test_string_literal() {
        let expr = parse_dsl("'hello'").unwrap();
        assert_eq!(expr, DslExpr::StringLit("hello".to_string()));
    }

    #[test]
    fn test_boolean_literals() {
        assert_eq!(parse_dsl("TRUE").unwrap(), DslExpr::Bool(true));
        assert_eq!(parse_dsl("FALSE").unwrap(), DslExpr::Bool(false));
    }

    #[test]
    fn test_null_literal() {
        assert_eq!(parse_dsl("NULL").unwrap(), DslExpr::Null);
    }

    #[test]
    fn test_negation() {
        let expr = parse_dsl("-amount").unwrap();
        assert!(matches!(expr, DslExpr::Negate(_)));
    }

    #[test]
    fn test_comparison() {
        let expr = parse_dsl("amount > 100").unwrap();
        match &expr {
            DslExpr::BinaryOp { op: BinaryOp::Gt, .. } => {}
            _ => panic!("expected Gt"),
        }
    }

    #[test]
    fn test_logical_operators() {
        let expr = parse_dsl("a > 0 AND b < 10").unwrap();
        match &expr {
            DslExpr::BinaryOp { op: BinaryOp::And, .. } => {}
            _ => panic!("expected And"),
        }
    }

    #[test]
    fn test_empty_expression_error() {
        assert!(parse_dsl("").is_err());
    }

    #[test]
    fn test_date_trunc_function() {
        let expr = parse_dsl("DATE_TRUNC(order_date, 'month')").unwrap();
        match &expr {
            DslExpr::FunctionCall(fc) => {
                assert_eq!(fc.name, "DATE_TRUNC");
                assert_eq!(fc.args.len(), 2);
            }
            _ => panic!("expected FunctionCall"),
        }
    }
}
