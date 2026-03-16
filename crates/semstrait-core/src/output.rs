//! Output types for the compilation pipeline.
//!
//! `CompiledPlan` is the main return type from the compiler, containing
//! the logical plan, optional SQL and Substrait representations, and metadata.

use crate::diagnostics::Diagnostic;

/// Options controlling compilation output.
#[derive(Debug, Clone)]
pub struct CompileOpts {
    /// SQL dialect for the emitter.
    pub dialect: Dialect,
    /// Whether to include SQL in the output.
    pub emit_sql: bool,
    /// Whether to include Substrait bytes in the output.
    pub emit_substrait: bool,
}

impl Default for CompileOpts {
    fn default() -> Self {
        Self {
            dialect: Dialect::Ansi,
            emit_sql: true,
            emit_substrait: false,
        }
    }
}

/// SQL dialect selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    /// ANSI standard SQL.
    Ansi,
    /// PostgreSQL-specific syntax.
    Postgres,
    /// BigQuery-specific syntax.
    BigQuery,
    /// Snowflake-specific syntax.
    Snowflake,
}

/// Output column metadata.
#[derive(Debug, Clone)]
pub struct OutputColumn {
    /// Semantic name (as requested by the user).
    pub name: String,
    /// Data type of the column.
    pub data_type: String,
    /// Whether this is a dimension or measure.
    pub role: ColumnRole,
}

/// Whether a column is a dimension, measure, or metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnRole {
    Dimension,
    Measure,
    Metric,
}

/// The result of compiling a semantic query.
#[derive(Debug)]
pub struct CompiledPlan {
    /// The SQL string (if `emit_sql` was set).
    pub sql: Option<String>,
    /// Substrait plan bytes (if `emit_substrait` was set).
    pub substrait: Option<Vec<u8>>,
    /// Output column metadata.
    pub columns: Vec<OutputColumn>,
    /// Non-fatal warnings produced during compilation.
    pub warnings: Vec<Diagnostic>,
}
