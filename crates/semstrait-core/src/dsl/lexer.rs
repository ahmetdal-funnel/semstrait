//! Lexer wrapper around logos tokenizer.

use logos::Logos;

use super::token::Token;

/// A positioned token with its span in the source string.
#[derive(Debug, Clone)]
pub struct SpannedToken {
    pub token: Token,
    pub span: std::ops::Range<usize>,
}

/// Tokenize a DSL expression string into a vector of spanned tokens.
pub fn tokenize(input: &str) -> Result<Vec<SpannedToken>, LexError> {
    let mut lexer = Token::lexer(input);
    let mut tokens = Vec::new();

    while let Some(result) = lexer.next() {
        match result {
            Ok(token) => {
                tokens.push(SpannedToken {
                    token,
                    span: lexer.span(),
                });
            }
            Err(()) => {
                return Err(LexError {
                    span: lexer.span(),
                    source: input.to_string(),
                });
            }
        }
    }

    Ok(tokens)
}

/// Error from tokenization — an unrecognized character sequence.
#[derive(Debug, Clone)]
pub struct LexError {
    pub span: std::ops::Range<usize>,
    pub source: String,
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let fragment = &self.source[self.span.clone()];
        write!(
            f,
            "unexpected character(s) '{}' at position {}",
            fragment, self.span.start
        )
    }
}

impl std::error::Error for LexError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_expression() {
        let tokens = tokenize("SUM(amount)").unwrap();
        assert_eq!(tokens.len(), 4); // SUM ( amount )
        assert!(tokens[0].token.is_keyword("sum"));
    }

    #[test]
    fn test_arithmetic() {
        let tokens = tokenize("revenue / users").unwrap();
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[1].token, Token::Slash);
    }

    #[test]
    fn test_string_literal() {
        let tokens = tokenize("'cancelled'").unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token, Token::StringLit("cancelled".to_string()));
    }

    #[test]
    fn test_comparison() {
        let tokens = tokenize("status != 'cancelled'").unwrap();
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[1].token, Token::NotEq);
    }

    #[test]
    fn test_qualified_name() {
        let tokens = tokenize("orders.amount").unwrap();
        assert_eq!(tokens.len(), 3); // orders . amount
        assert_eq!(tokens[1].token, Token::Dot);
    }

    #[test]
    fn test_number_literal() {
        let tokens = tokenize("3.14").unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token, Token::Number(3.14));
    }
}
