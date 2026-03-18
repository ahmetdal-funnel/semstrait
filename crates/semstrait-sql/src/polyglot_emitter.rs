//! Polyglot-sql backed SQL emitter.
//!
//! Uses the existing `AnsiSqlEmitter` to generate ANSI SQL, then transpiles
//! it to the target dialect via `polyglot_sql::transpile()`. This gives us
//! dialect-aware output (identifier quoting, LIMIT vs FETCH FIRST, etc.)
//! for 34+ SQL dialects without per-dialect `SqlDialect` implementations.

use crate::dialect::{AnsiDialect, SqlDialect, TargetDialect};
use crate::emitter::{AnsiSqlEmitter, SqlEmitter};
use crate::error::EmitError;
use polyglot_sql::DialectType;
use semstrait_ir::LogicalPlan;

impl TargetDialect {
    /// Convert to polyglot-sql's `DialectType`. Returns `None` for ANSI
    /// (pass-through — no transpilation needed).
    fn to_polyglot(self) -> Option<DialectType> {
        match self {
            Self::Ansi => None,
            Self::DataFusion => Some(DialectType::DataFusion),
            Self::DuckDb => Some(DialectType::DuckDB),
            Self::Trino => Some(DialectType::Trino),
            Self::Spark => Some(DialectType::Spark),
            Self::Snowflake => Some(DialectType::Snowflake),
            Self::Databricks => Some(DialectType::Databricks),
            Self::PostgreSql => Some(DialectType::PostgreSQL),
        }
    }
}

/// SQL emitter that generates dialect-specific SQL via polyglot-sql transpilation.
///
/// Architecture:
/// ```text
/// PlanNode -> AnsiSqlEmitter (ANSI SQL string) -> polyglot::transpile -> target dialect SQL
/// ```
///
/// The ANSI SQL is generated using our existing, well-tested `AnsiSqlEmitter`.
/// Polyglot-sql then handles dialect-specific transformations:
/// - Identifier quoting (double-quotes → backticks for Spark/Databricks)
/// - FETCH FIRST → LIMIT conversion
/// - Function name normalization
pub struct PolyglotEmitter {
    base: AnsiSqlEmitter<AnsiDialect>,
    target: TargetDialect,
}

impl PolyglotEmitter {
    pub fn new(target: TargetDialect) -> Self {
        Self {
            base: AnsiSqlEmitter::new(AnsiDialect),
            target,
        }
    }

    /// Transpile ANSI SQL to the target dialect.
    fn transpile(&self, ansi_sql: &str) -> Result<String, EmitError> {
        match self.target.to_polyglot() {
            None => Ok(ansi_sql.to_string()),
            Some(target_dialect) => {
                // Parse as DuckDB (lenient parser that handles ANSI + common extensions)
                // and generate for the target dialect.
                let results = polyglot_sql::transpile(ansi_sql, DialectType::DuckDB, target_dialect)
                    .map_err(|e| EmitError::InvalidPlan(format!("transpile failed: {e}")))?;
                results
                    .into_iter()
                    .next()
                    .ok_or_else(|| EmitError::InvalidPlan("transpile returned no output".into()))
            }
        }
    }
}

impl SqlEmitter for PolyglotEmitter {
    fn emit(&self, plan: &LogicalPlan) -> Result<String, EmitError> {
        let ansi_sql = self.base.emit(plan)?;
        self.transpile(&ansi_sql)
    }

    fn dialect(&self) -> &dyn SqlDialect {
        self.base.dialect()
    }
}
