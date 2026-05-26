//! Byte-blob transport. Spec `31b`.
//!
//! Public surface: [`Source`], [`Sink`], [`FromIoBytes`], [`IntoIoBytes`],
//! [`Location`], [`IoErrorKind`]. Back-ends are reached through
//! [`backends`]; no back-end type re-exports at this level (`31b §2`).

pub mod backends;
mod convert;
mod error;
mod location;
mod sink;
mod source;

pub use crate::io::convert::{FromIoBytes, IntoIoBytes};
pub use crate::io::error::IoErrorKind;
pub use crate::io::location::Location;
pub use crate::io::sink::Sink;
pub use crate::io::source::Source;
