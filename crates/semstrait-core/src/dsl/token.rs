//! DSL token definitions for the logos-based lexer.

use logos::Logos;

#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\r\n]+")]
pub enum Token {
    // Literals
    #[regex(r"[0-9]+(\.[0-9]+)?", |lex| lex.slice().parse::<f64>().ok())]
    Number(f64),

    #[regex(r"'[^']*'", |lex| {
        let s = lex.slice();
        Some(s[1..s.len()-1].to_string())
    })]
    StringLit(String),

    // Identifiers and keywords (case-insensitive keywords handled in parser)
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", |lex| lex.slice().to_string())]
    Ident(String),

    // Operators
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("=")]
    Eq,
    #[token("!=")]
    NotEq,
    #[token("<>")]
    LtGt,
    #[token("<")]
    Lt,
    #[token("<=")]
    LtEq,
    #[token(">")]
    Gt,
    #[token(">=")]
    GtEq,

    // Punctuation
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token(",")]
    Comma,
    #[token(".")]
    Dot,
}

impl Token {
    /// Check if this token is a keyword (case-insensitive).
    #[allow(dead_code)]
    pub fn is_keyword(&self, kw: &str) -> bool {
        match self {
            Token::Ident(s) => s.eq_ignore_ascii_case(kw),
            _ => false,
        }
    }
}
