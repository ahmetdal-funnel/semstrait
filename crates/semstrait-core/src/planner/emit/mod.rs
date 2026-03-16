//! Plan emission — PlanNode to output formats.
//!
//! Each emitter implements the [`PlanEmitter`] trait, converting a PlanNode tree
//! into a specific output format (SQL string, Substrait protobuf, etc.).

pub mod sql;
pub mod substrait;

use crate::planner::ir::error::EmitError;
use crate::planner::ir::plan_node::PlanNode;

#[allow(dead_code)] // API surface for future use
/// Trait for converting a PlanNode tree into an output format.
///
/// Implementations:
/// - [`SqlEmitter`] → ANSI SQL string
/// - [`SubstraitEmitter`] → `substrait::proto::Plan` protobuf
pub trait PlanEmitter {
    /// The output type produced by this emitter.
    type Output;

    /// Emit the plan as the target format.
    ///
    /// `output_names` optionally renames the outermost columns.
    fn emit(
        &self,
        plan: &PlanNode,
        output_names: Option<Vec<String>>,
    ) -> Result<Self::Output, EmitError>;
}

/// SQL emitter — produces ANSI SQL strings.
#[allow(dead_code)]
pub struct SqlEmitter;

impl PlanEmitter for SqlEmitter {
    type Output = String;

    fn emit(
        &self,
        plan: &PlanNode,
        output_names: Option<Vec<String>>,
    ) -> Result<Self::Output, EmitError> {
        sql::emit_sql(plan, output_names)
    }
}

/// Substrait emitter — produces Substrait protobuf plans.
#[allow(dead_code)]
pub struct SubstraitEmitter;

impl PlanEmitter for SubstraitEmitter {
    type Output = ::substrait::proto::Plan;

    fn emit(
        &self,
        plan: &PlanNode,
        output_names: Option<Vec<String>>,
    ) -> Result<Self::Output, EmitError> {
        substrait::emit_plan(plan, output_names)
    }
}
