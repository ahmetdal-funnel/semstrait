//! Polyglot-sql backed SQL emitter.
//!
//! Builds a polyglot-sql AST from the `PlanNode` IR via the builder API, then
//! renders to the target dialect via `polyglot_sql::generate()`.

use crate::dialect::{AnsiDialect, SqlDialect, TargetDialect};
use crate::emitter::SqlEmitter;
use crate::error::EmitError;
use crate::polyglot::PlanBuilder;
use polyglot_sql::DialectType;
use semstrait_ir::LogicalPlan;

impl TargetDialect {
    fn to_polyglot(self) -> DialectType {
        match self {
            Self::Ansi => DialectType::Generic,
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
/// ```text
/// PlanNode → PlanBuilder → Expression AST → polyglot::generate → target SQL
/// ```
pub struct PolyglotEmitter {
    builder: PlanBuilder,
    target: TargetDialect,
    ansi_dialect: AnsiDialect,
}

impl PolyglotEmitter {
    pub fn new(target: TargetDialect) -> Self {
        Self {
            builder: PlanBuilder::new(),
            target,
            ansi_dialect: AnsiDialect,
        }
    }
}

impl SqlEmitter for PolyglotEmitter {
    fn emit(&self, plan: &LogicalPlan) -> Result<String, EmitError> {
        let ast = self.builder.build(plan)?;
        polyglot_sql::generate(&ast, self.target.to_polyglot())
            .map_err(|e| EmitError::InvalidPlan(format!("generate failed: {e}")))
    }

    fn dialect(&self) -> &dyn SqlDialect {
        &self.ansi_dialect
    }
}
