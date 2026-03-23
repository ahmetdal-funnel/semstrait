//! semstrait - Compile semantic models to Substrait compute plans
//!
//! This library provides:
//! - Schema definition types (SemanticModel, GrainSet, Dimension, Measure, etc.)
//! - Schema parsing from YAML
//! - Dataset selection (aggregate awareness)
//! - Query resolution
//! - Logical plan generation
//! - Substrait plan emission
//!
//! # Architecture
//!
//! **Noun modules** (data structures):
//! - `semantic_model/` - domain concepts (Schema, SemanticModel, GrainSet, Dimension, Measure)
//! - `query/` - query request types (QueryRequest, DataFilter)
//! - `plan/` - logical plan types (PlanNode, Expr, Column)
//!
//! **Verb modules** (transformations):
//! - `parser/` - YAML → Schema
//! - `selector/` - SemanticModel + Query → Selected datasets (aggregate awareness)
//! - `resolver/` - Schema + QueryRequest + Datasets → ResolvedQuery
//! - `planner/` - ResolvedQuery → PlanNode
//! - `emitter/` - PlanNode → Substrait or SQL
//!
//! # Example
//!
//! ```ignore
//! use semstrait::{parser, select_datasets, resolve_query, plan_query, emit_plan, QueryRequest};
//!
//! let schema = parser::parse_file("schema.yaml")?;
//! let request = QueryRequest { model: "sales".into(), ..Default::default() };
//! let model = schema.get_model(&request.model).unwrap();
//! let grain_sets = model.grain_sets();
//! let datasets = select_datasets(&schema, model, &grain_sets, &dims, &measures)?;
//! let resolved = resolve_query(&schema, &request, &datasets[0])?;
//! let plan = plan_query(&resolved)?;
//! let substrait = emit_plan(&plan)?;
//! ```

pub mod emitter;
pub mod error;
pub mod parser;
pub mod plan;
pub mod planner;
pub mod query;
pub mod resolver;
pub mod selector;
pub mod semantic_model;

// Re-export commonly used types
pub use emitter::{emit_plan, emit_sql, EmitError};
pub use error::ParseError;
pub use plan::{AggregateExpr, Column, Expr, PlanNode};
pub use planner::{plan_query, plan_semantic_query, PlanError};
pub use query::{DataFilter, QueryRequest};
pub use resolver::{resolve_query, ResolveError, ResolvedQuery};
pub use selector::{select_datasets, SelectError, SelectedDataset};
pub use semantic_model::{
    resolve_dimension_path_template, resolve_path_template, Aggregation, DataType, Dataset,
    GrainSet, GrainSetDimension, Schema, SemanticModel,
};
