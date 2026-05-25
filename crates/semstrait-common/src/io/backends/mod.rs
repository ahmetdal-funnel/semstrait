//! Per `31b §8`. Back-end roster — `memory` + `local` always present
//! under `io`; `s3` lives behind `io-aws`.

pub mod local;
pub mod memory;

#[cfg(feature = "io-aws")]
pub mod s3;
