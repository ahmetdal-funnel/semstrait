//! Plan-tree module. Spec `35 §10` / `§11.1` / `§14`.
//!
//! Organization:
//! - [`node`] — [`PlanNode`] sum + per-variant struct payloads
//!   ([`ScanNode`], [`FilterNode`], [`ProjectNode`], [`AggNode`],
//!   [`JoinNode`], [`UnionNode`], [`SortNode`], [`FetchNode`],
//!   [`ValuesNode`]). Per `35 §10`.
//! - [`meta`] — [`NodeMeta`], [`NodeId`], [`SemAnnotation`],
//!   [`BoundaryPosition`]. Per `35 §11.1`.
//! - [`traversal`] — [`crate::tree::Tree`] impl on [`PlanNode`]. Provides
//!   `apply` / `transform` walks via the universal-traversal trait
//!   family. Per `35 §14`.
//! - [`validate`] — [`SemanticPlan`] wrapper + `validate()` post-order
//!   walker. Per `35 §17.1` (first-violation-wins) and `§13` invariants
//!   that ground in IR-only state.

pub mod meta;
pub mod node;
pub mod traversal;
pub mod validate;

pub use meta::{AnnotationClass, BoundaryPosition, NodeId, NodeMeta, SemAnnotation};
pub use node::{
    AggNode, FetchNode, FilterNode, JoinNode, PlanNode, ProjectNode, ScanNode, SortNode,
    UnionNode, ValuesNode,
};
pub use validate::SemanticPlan;
