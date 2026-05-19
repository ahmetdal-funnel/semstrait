//! YAML authoring-surface parser.
//!
//! Phase 8 refactor (`STATUS.md` item Q follow-up): the legacy
//! `expr_source/deserialize.rs` (1700+ lines) is decomposed into
//! reusable modules, each owning one concern:
//!
//! - [`error`] — unified [`ParseError`] roster (block + mapping).
//! - [`token`] — the [`tokenize_leaf`](token::tokenize_leaf) shared by
//!   every leaf-position scalar; output is [`token::LeafToken`]. The
//!   future inline-DSL parser (`14 §6.3`) reuses the same tokenizer
//!   so block + inline produce identical [`semstrait_ir::Expr<L>`].
//! - [`sugar`] — closed sugar tables. Author-visible operator and
//!   function-call tags live here; the dispatcher in [`block`]
//!   short-circuits through them.
//! - [`leaf`] — sealed [`LeafResolver`] trait + impls for the two
//!   canonical leaf sets ([`semstrait_ir::SemanticLeaf`] and
//!   [`semstrait_ir::PhysicalLeaf`]) plus the body parsers for the
//!   semantic-tag family (`field` / `dim` / `measure` / `metric` /
//!   `key`).
//! - [`block`] — recursive-descent over `serde_yaml::Value` for
//!   block-form expressions (Phase 8 Pass B).
//! - [`mapping`] — `semantic_mapping:` value parser
//!   (bare = `Column`, `lit:` = `Literal`,
//!   `expr:` = `ExprSource<PhysicalLeaf>`) (Phase 8 Pass B).
//!
//! Public re-exports collapse the surface: callers `use parser::{...}`
//! to reach the entry points.

pub mod block;
pub mod error;
pub mod leaf;
pub mod mapping;
pub mod sugar;
pub mod token;

pub use block::parse_block;
pub use error::ParseError;
pub use leaf::LeafResolver;
pub use mapping::deserialize_mapping_value;
pub use token::{tokenize_leaf, LeafToken};
