//! API layer: gRPC, REST, and CLI entry points for semstrait.
//!
//! All transports share `RequestParser` and `SemstraitEngine`.
//! Submodules are feature-gated: cli, rest, grpc.

pub mod engine;
pub mod error;
pub mod parse;
pub mod types;

#[cfg(feature = "cli")]
pub mod cli;

#[cfg(feature = "rest")]
pub mod rest;

#[cfg(feature = "grpc")]
pub mod grpc;

pub use engine::SemstraitEngine;
pub use error::EngineError;
pub use parse::RequestParser;
pub use types::{
    ExplainResult, QueryRequest, RawQueryRequest, ValidationResult,
};
