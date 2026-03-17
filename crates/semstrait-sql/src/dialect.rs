//! SQL dialect trait and implementations.

use semstrait_core::Grain;

/// Dialect-specific SQL generation behavior.
///
/// Each dialect knows how to quote identifiers, format date truncation,
/// and handle engine-specific SQL idioms.
pub trait SqlDialect: Send + Sync {
    /// Quote an identifier (column or table name) per dialect rules.
    fn quote_identifier(&self, ident: &str) -> String;

    /// Whether this dialect supports CTEs (WITH clauses).
    fn supports_cte(&self) -> bool;

    /// Generate a DATE_TRUNC expression for the given grain and inner expression.
    fn date_trunc(&self, grain: &Grain, expr: &str) -> String;

    /// Generate a null-safe equality comparison.
    fn null_safe_eq(&self, l: &str, r: &str) -> String;

    /// The expression for the current timestamp.
    fn current_timestamp(&self) -> String;

    /// Generate a ROW_NUMBER() window function expression.
    fn window_row_number(&self, partition_by: &[&str], order_by: &str) -> String;

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
    fn quote_identifier(&self, ident: &str) -> String {
        // Escape any existing double quotes by doubling them
        format!("\"{}\"", ident.replace('"', "\"\""))
    }

    fn supports_cte(&self) -> bool {
        true
    }

    fn date_trunc(&self, grain: &Grain, expr: &str) -> String {
        format!("DATE_TRUNC('{}', {})", grain, expr)
    }

    fn null_safe_eq(&self, l: &str, r: &str) -> String {
        format!("({l} IS NOT DISTINCT FROM {r})")
    }

    fn current_timestamp(&self) -> String {
        "CURRENT_TIMESTAMP".to_string()
    }

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
    fn quote_identifier(&self, ident: &str) -> String {
        // DuckDB uses ANSI-standard double-quoted identifiers
        format!("\"{}\"", ident.replace('"', "\"\""))
    }

    fn supports_cte(&self) -> bool {
        true
    }

    fn date_trunc(&self, grain: &Grain, expr: &str) -> String {
        // DuckDB uses date_trunc('grain', expr) same as ANSI but lowercase function
        format!("date_trunc('{}', {})", grain, expr)
    }

    fn null_safe_eq(&self, l: &str, r: &str) -> String {
        format!("({l} IS NOT DISTINCT FROM {r})")
    }

    fn current_timestamp(&self) -> String {
        "current_timestamp".to_string()
    }

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

/// Trino dialect. Double-quoted identifiers, Trino-specific date_trunc.
pub struct TrinoDialect;

impl SqlDialect for TrinoDialect {
    fn quote_identifier(&self, ident: &str) -> String {
        format!("\"{}\"", ident.replace('"', "\"\""))
    }

    fn supports_cte(&self) -> bool {
        true
    }

    fn date_trunc(&self, grain: &Grain, expr: &str) -> String {
        // Trino uses date_trunc('grain', expr) with lowercase grain
        format!("date_trunc('{}', {})", grain, expr)
    }

    fn null_safe_eq(&self, l: &str, r: &str) -> String {
        format!("({l} IS NOT DISTINCT FROM {r})")
    }

    fn current_timestamp(&self) -> String {
        "current_timestamp".to_string()
    }

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

    fn limit_clause(&self, count: Option<i64>, offset: i64) -> String {
        // Trino uses ANSI FETCH FIRST syntax
        match (count, offset) {
            (Some(c), 0) => format!("FETCH FIRST {c} ROWS ONLY"),
            (Some(c), o) => format!("OFFSET {o} ROWS FETCH FIRST {c} ROWS ONLY"),
            (None, o) if o > 0 => format!("OFFSET {o} ROWS"),
            _ => String::new(),
        }
    }
}
