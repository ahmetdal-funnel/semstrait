//! YAML-facing intermediate types for `parse`.
//!
//! The canonical in-memory [`crate::SemanticModel`] keys top-level
//! data-kind / semantics maps as `BTreeMap<String, _>`, but YAML
//! authors write **arrays** of named entries per `32 §1`. This module
//! holds the YAML-array form together with conversion helpers that
//! produce the canonical form while reporting per-source diagnostics
//! (duplicate names, etc.).

pub mod env;
pub mod root;
pub(crate) mod tagged;

pub(crate) use root::YamlRoot;
