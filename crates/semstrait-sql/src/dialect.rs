//! SQL dialect trait and implementations.

use semstrait_core::Grain;

/// Dialect-specific SQL generation behavior.
///
/// Each dialect knows how to quote identifiers, format date truncation,
/// and handle engine-specific SQL idioms.
pub trait SqlDialect: Send + Sync {
    /// Quote an identifier (column or table name) per dialect rules.
    ///
    /// Default: ANSI double-quoted identifiers with escaped inner quotes.
    fn quote_identifier(&self, ident: &str) -> String {
        format!("\"{}\"", ident.replace('"', "\"\""))
    }

    /// Whether this dialect supports CTEs (WITH clauses).
    fn supports_cte(&self) -> bool {
        true
    }

    /// Generate a DATE_TRUNC expression for the given grain and inner expression.
    fn date_trunc(&self, grain: &Grain, expr: &str) -> String;

    /// Generate a null-safe equality comparison.
    ///
    /// Default: ANSI `IS NOT DISTINCT FROM`.
    fn null_safe_eq(&self, l: &str, r: &str) -> String {
        format!("({l} IS NOT DISTINCT FROM {r})")
    }

    /// The expression for the current timestamp.
    fn current_timestamp(&self) -> String;

    /// Generate a ROW_NUMBER() window function expression.
    ///
    /// Default: standard `ROW_NUMBER() OVER (PARTITION BY ... ORDER BY ...)`.
    fn window_row_number(&self, partition_by: &[&str], order_by: &str) -> String {
        if partition_by.is_empty() {
            format!("ROW_NUMBER() OVER (ORDER BY {order_by})")
        } else {
            format!(
                "ROW_NUMBER() OVER (PARTITION BY {} ORDER BY {order_by})",
                partition_by.join(", ")
            )
        }
    }

    /// Generate a LIMIT/FETCH clause for row-limiting.
    /// Returns an empty string if both count is None and offset is 0.
    fn limit_clause(&self, count: Option<i64>, offset: i64) -> String;
}

// =============================================================================
// AnsiDialect — safe baseline, double-quoted identifiers
// =============================================================================

/// ANSI SQL dialect. Double-quoted identifiers, standard DATE_TRUNC.
pub struct AnsiDialect;

impl SqlDialect for AnsiDialect {
    fn date_trunc(&self, grain: &Grain, expr: &str) -> String {
        format!("DATE_TRUNC('{}', {})", grain, expr)
    }

    fn current_timestamp(&self) -> String {
        "CURRENT_TIMESTAMP".to_string()
    }

    fn limit_clause(&self, count: Option<i64>, offset: i64) -> String {
        match (count, offset) {
            (Some(c), 0) => format!("FETCH FIRST {c} ROWS ONLY"),
            (Some(c), o) => format!("OFFSET {o} ROWS FETCH FIRST {c} ROWS ONLY"),
            (None, o) if o > 0 => format!("OFFSET {o} ROWS"),
            _ => String::new(),
        }
    }
}

// =============================================================================
// DuckDbDialect — double-quoted identifiers (ANSI), DuckDB-specific date_trunc
// =============================================================================

/// DuckDB dialect. Double-quoted identifiers (ANSI standard), DuckDB date_trunc syntax.
pub struct DuckDbDialect;

impl SqlDialect for DuckDbDialect {
    fn date_trunc(&self, grain: &Grain, expr: &str) -> String {
        format!("date_trunc('{}', {})", grain, expr)
    }

    fn current_timestamp(&self) -> String {
        "current_timestamp".to_string()
    }

    fn limit_clause(&self, count: Option<i64>, offset: i64) -> String {
        match (count, offset) {
            (Some(c), 0) => format!("LIMIT {c}"),
            (Some(c), o) => format!("LIMIT {c} OFFSET {o}"),
            (None, o) if o > 0 => format!("OFFSET {o}"),
            _ => String::new(),
        }
    }
}

// =============================================================================
// TrinoDialect — double-quoted identifiers, Trino date_trunc syntax
// =============================================================================

// =============================================================================
// TargetDialect — engine-agnostic dialect identifier
// =============================================================================

/// Target SQL dialect identifier.
///
/// Used by connectors to declare their preferred dialect and by the engine
/// to select the appropriate SQL emitter. When the `polyglot` feature is
/// enabled, `PolyglotEmitter` uses this to pick the transpilation target.
/// Without `polyglot`, the engine falls back to the matching `SqlDialect`
/// implementation (ANSI, DuckDB, or Trino).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetDialect {
    /// ANSI SQL (pass-through, no transpilation).
    Ansi,
    /// Apache DataFusion.
    DataFusion,
    /// DuckDB.
    DuckDb,
    /// Trino (formerly PrestoSQL).
    Trino,
    /// Apache Spark SQL.
    Spark,
    /// Snowflake.
    Snowflake,
    /// Databricks SQL.
    Databricks,
    /// PostgreSQL.
    PostgreSql,
}

// =============================================================================
// TrinoDialect — double-quoted identifiers, Trino date_trunc syntax
// =============================================================================

/// Trino dialect. Double-quoted identifiers, Trino-specific date_trunc.
pub struct TrinoDialect;

impl SqlDialect for TrinoDialect {
    fn date_trunc(&self, grain: &Grain, expr: &str) -> String {
        format!("date_trunc('{}', {})", grain, expr)
    }

    fn current_timestamp(&self) -> String {
        "current_timestamp".to_string()
    }

    fn limit_clause(&self, count: Option<i64>, offset: i64) -> String {
        match (count, offset) {
            (Some(c), 0) => format!("FETCH FIRST {c} ROWS ONLY"),
            (Some(c), o) => format!("OFFSET {o} ROWS FETCH FIRST {c} ROWS ONLY"),
            (None, o) if o > 0 => format!("OFFSET {o} ROWS"),
            _ => String::new(),
        }
    }
}
