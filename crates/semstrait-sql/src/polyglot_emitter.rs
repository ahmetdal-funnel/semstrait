//! Polyglot-sql backed SQL emitter.
//!
//! Builds a polyglot-sql AST from the `PlanNode` IR via the builder API, then
//! renders to the target dialect via `polyglot_sql::generate()`. This gives us
//! dialect-aware output (identifier quoting, LIMIT vs FETCH FIRST, etc.)
//! for 34+ SQL dialects without per-dialect `SqlDialect` implementations.

use crate::dialect::{AnsiDialect, SqlDialect, TargetDialect};
use crate::emitter::SqlEmitter;
use crate::error::EmitError;
use crate::polyglot::PlanBuilder;
use polyglot_sql::DialectType;
use semstrait_ir::LogicalPlan;

impl TargetDialect {
    /// Convert to polyglot-sql's `DialectType`.
    fn to_polyglot(self) -> DialectType {
        match self {
            Self::Ansi => DialectType::Generic, // Standard SQL with no dialect-specific behavior
            Self::DataFusion => DialectType::DataFusion,
            Self::DuckDb => DialectType::DuckDB,
            Self::Trino => DialectType::Trino,
            Self::Spark => DialectType::Spark,
            Self::Snowflake => DialectType::Snowflake,
            Self::Databricks => DialectType::Databricks,
            Self::PostgreSql => DialectType::PostgreSQL,
        }
    }
}

/// SQL emitter that generates dialect-specific SQL via polyglot-sql AST builder.
///
/// Architecture:
/// ```text
/// PlanNode -> PlanBuilder (AST construction) -> Expression -> polyglot::generate -> target SQL
/// ```
///
/// The PlanNode IR is converted to a polyglot-sql Expression AST using the
/// programmatic builder API. Polyglot-sql then renders dialect-specific SQL:
/// - Identifier quoting (double-quotes → backticks for Spark/Databricks)
/// - FETCH FIRST → LIMIT conversion
/// - Function name normalization
pub struct PolyglotEmitter {
    builder: PlanBuilder,
    target: TargetDialect,
    /// Kept for backward compatibility with `SqlEmitter::dialect()`.
    _ansi_dialect: AnsiDialect,
}

impl PolyglotEmitter {
    pub fn new(target: TargetDialect) -> Self {
        Self {
            builder: PlanBuilder::new(),
            target,
            _ansi_dialect: AnsiDialect,
        }
    }
}

impl SqlEmitter for PolyglotEmitter {
    fn emit(&self, plan: &LogicalPlan) -> Result<String, EmitError> {
        // Build polyglot-sql AST from PlanNode IR
        let ast = self.builder.build(plan)?;

        // Generate SQL for the target dialect
        let dialect = self.target.to_polyglot();
        polyglot_sql::generate(&ast, dialect)
            .map_err(|e| EmitError::InvalidPlan(format!("generate failed: {e}")))
    }

    fn dialect(&self) -> &dyn SqlDialect {
        &self._ansi_dialect
    }
}
