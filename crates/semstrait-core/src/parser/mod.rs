//! YAML semantic model parser.
//!
//! Loads semantic model definitions from YAML files or strings,
//! validates structural correctness, resolves `ref:` entries,
//! and enforces nesting matrix rules.

mod load;
mod nesting;
mod refs;

pub use load::{parse_file, parse_str, ParseError};
