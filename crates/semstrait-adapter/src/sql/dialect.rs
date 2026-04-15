//! SQL dialect trait and implementations.

use semstrait_core::{DataType, Grain};

/// Dialect-specific SQL generation behavior.
///
/// Each dialect knows how to quote identifiers, format date truncation,
/// and handle engine-specific SQL idioms.
pub trait SqlDialect: Send + Sync {
    /// Map a canonical `DataType` to the engine-specific SQL type name.
    ///
    /// Default: ANSI SQL type names. Engine dialects override for their syntax.
    fn type_name(&self, dt: &DataType) -> String {
        match dt {
            DataType::Integer => "INTEGER".into(),
            DataType::Number => "DOUBLE PRECISION".into(),
            DataType::Decimal { precision, scale } => format!("DECIMAL({precision},{scale})"),
            DataType::String => "VARCHAR".into(),
            DataType::Boolean => "BOOLEAN".into(),
            DataType::Date => "DATE".into(),
            DataType::Timestamp { precision } => format!("TIMESTAMP({precision})"),
            DataType::Binary => "VARBINARY".into(),
        }
    }

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

    /// Case-insensitive LIKE. Pre-rendered `expr` and `pattern` SQL fragments.
    ///
    /// Default (ANSI): lowercases both sides since ILIKE is non-standard.
    /// Engines with native ILIKE (DataFusion, DuckDB) override to use it directly.
    fn ilike(&self, expr: &str, pattern: &str) -> String {
        format!("LOWER({expr}) LIKE LOWER({pattern})")
    }

    /// Regex match predicate. Pre-rendered `expr` and `pattern` SQL fragments.
    ///
    /// `full_match`: when true, pattern must match entire string.
    /// Default (ANSI): `REGEXP_LIKE(expr, pattern)`.
    fn regexp_match(&self, expr: &str, pattern: &str, full_match: bool) -> String {
        // ANSI REGEXP_LIKE does substring match; anchor for full match.
        if full_match {
            format!("REGEXP_LIKE({expr}, CONCAT('^', {pattern}, '$'))")
        } else {
            format!("REGEXP_LIKE({expr}, {pattern})")
        }
    }

    /// Regex extract — return nth capture group from first match.
    /// Pre-rendered `expr` and `pattern` SQL fragments.
    ///
    /// Default (ANSI): `REGEXP_EXTRACT(expr, pattern, group_idx)`.
    fn regexp_extract(&self, expr: &str, pattern: &str, group_idx: usize) -> String {
        format!("REGEXP_EXTRACT({expr}, {pattern}, {group_idx})")
    }
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
// DataFusionDialect — LIMIT syntax, native ILIKE, standard DATE_TRUNC
// =============================================================================

/// DataFusion SQL dialect. LIMIT (not FETCH FIRST), native ILIKE.
#[cfg(feature = "datafusion")]
pub struct DataFusionDialect;

#[cfg(feature = "datafusion")]
impl SqlDialect for DataFusionDialect {
    fn type_name(&self, dt: &DataType) -> String {
        match dt {
            DataType::Integer => "BIGINT".into(),
            DataType::Number => "DOUBLE".into(),
            _ => AnsiDialect.type_name(dt),
        }
    }

    fn date_trunc(&self, grain: &Grain, expr: &str) -> String {
        format!("date_trunc('{}', {})", grain, expr)
    }

    fn current_timestamp(&self) -> String {
        "now()".to_string()
    }

    fn limit_clause(&self, count: Option<i64>, offset: i64) -> String {
        match (count, offset) {
            (Some(c), 0) => format!("LIMIT {c}"),
            (Some(c), o) => format!("LIMIT {c} OFFSET {o}"),
            (None, o) if o > 0 => format!("OFFSET {o}"),
            _ => String::new(),
        }
    }

    /// DataFusion supports ILIKE natively.
    fn ilike(&self, expr: &str, pattern: &str) -> String {
        format!("{expr} ILIKE {pattern}")
    }

    /// DataFusion `regexp_match` — returns array, check IS NOT NULL for predicate.
    fn regexp_match(&self, expr: &str, pattern: &str, full_match: bool) -> String {
        if full_match {
            format!("regexp_match({expr}, CONCAT('^', {pattern}, '$')) IS NOT NULL")
        } else {
            format!("regexp_match({expr}, {pattern}) IS NOT NULL")
        }
    }
}

// =============================================================================
// DuckDbDialect — double-quoted identifiers (ANSI), DuckDB-specific date_trunc
// =============================================================================

/// DuckDB dialect. Double-quoted identifiers (ANSI standard), DuckDB date_trunc syntax.
#[cfg(feature = "duckdb")]
pub struct DuckDbDialect;

#[cfg(feature = "duckdb")]
impl SqlDialect for DuckDbDialect {
    fn type_name(&self, dt: &DataType) -> String {
        match dt {
            DataType::Integer => "BIGINT".into(),
            DataType::Number => "DOUBLE".into(),
            _ => AnsiDialect.type_name(dt),
        }
    }

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

    /// DuckDB supports ILIKE natively.
    fn ilike(&self, expr: &str, pattern: &str) -> String {
        format!("{expr} ILIKE {pattern}")
    }

    /// DuckDB `regexp_matches` does substring match by default.
    fn regexp_match(&self, expr: &str, pattern: &str, full_match: bool) -> String {
        if full_match {
            format!("regexp_matches({expr}, CONCAT('^', {pattern}, '$'))")
        } else {
            format!("regexp_matches({expr}, {pattern})")
        }
    }

    /// DuckDB `regexp_extract(expr, pattern, group_idx)`.
    fn regexp_extract(&self, expr: &str, pattern: &str, group_idx: usize) -> String {
        format!("regexp_extract({expr}, {pattern}, {group_idx})")
    }
}

// =============================================================================
// SparkDialect — Spark SQL with LIMIT syntax
// =============================================================================

/// Spark SQL dialect. Double-quoted identifiers, LIMIT syntax.
#[cfg(feature = "spark")]
pub struct SparkDialect;

#[cfg(feature = "spark")]
impl SqlDialect for SparkDialect {
    fn type_name(&self, dt: &DataType) -> String {
        match dt {
            DataType::Integer => "BIGINT".into(),
            DataType::Number => "DOUBLE".into(),
            _ => AnsiDialect.type_name(dt),
        }
    }

    fn date_trunc(&self, grain: &Grain, expr: &str) -> String {
        format!("date_trunc('{}', {})", grain, expr)
    }

    fn current_timestamp(&self) -> String {
        "current_timestamp()".to_string()
    }

    fn limit_clause(&self, count: Option<i64>, offset: i64) -> String {
        match (count, offset) {
            (Some(c), 0) => format!("LIMIT {c}"),
            (Some(c), o) => format!("LIMIT {c} OFFSET {o}"),
            (None, o) if o > 0 => format!("OFFSET {o}"),
            _ => String::new(),
        }
    }

    /// Spark has no ILIKE — lower both sides.
    fn ilike(&self, expr: &str, pattern: &str) -> String {
        format!("LOWER({expr}) LIKE LOWER({pattern})")
    }

    /// Spark `RLIKE` does full-string match natively.
    fn regexp_match(&self, expr: &str, pattern: &str, full_match: bool) -> String {
        if full_match {
            format!("{expr} RLIKE {pattern}")
        } else {
            // For substring match on Spark, wrap with .*
            format!("{expr} RLIKE CONCAT('.*', {pattern}, '.*')")
        }
    }

    /// Spark `regexp_extract(expr, pattern, group_idx)`.
    fn regexp_extract(&self, expr: &str, pattern: &str, group_idx: usize) -> String {
        format!("regexp_extract({expr}, {pattern}, {group_idx})")
    }
}

// =============================================================================
// TargetDialect — engine-agnostic dialect identifier
// =============================================================================

/// Target SQL dialect identifier.
///
/// Used by connectors to declare their preferred dialect and by the engine
/// to select the appropriate SQL emitter. When the `polyglot` feature is
/// enabled, `PolyglotEmitter` uses this to pick the transpilation target.
/// Without `polyglot`, the engine falls back to the matching `SqlDialect`
/// implementation (ANSI, DuckDB, or Spark).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetDialect {
    /// ANSI SQL (pass-through, no transpilation).
    Ansi,
    /// Apache DataFusion.
    DataFusion,
    /// DuckDB.
    DuckDb,
    /// Apache Spark SQL.
    Spark,
    /// Snowflake.
    Snowflake,
    /// Databricks SQL.
    Databricks,
    /// PostgreSQL.
    PostgreSql,
}
