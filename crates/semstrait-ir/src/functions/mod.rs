//! Canonical function catalog. Per spec `35 §8` and foundations
//! `14a §2`–`§7`.
//!
//! Owns:
//! - [`CanonicalFn`] — newtype-over-stable identity per `35 §8.2`.
//! - [`FunctionRegistry`] + [`function_registry`] — process-wide
//!   sealed singleton per `14a §2.1`.
//! - [`FunctionSpec`] / [`FnSignature`] / [`ParamType`] /
//!   [`ReturnTypeRule`] / [`FunctionCategory`] — per-function
//!   declaration shape per `35 §8.2` / `14a §3`.
//! - [`Additivity`] / [`DimensionAxis`] — function-level additivity
//!   carrier per `14a §3.6`. Composed downstream with model-level
//!   `AdditivityType` per `19 §6.5`.
//! - [`RegistryExtension`] — adapter extension hook per `14a §7`.
//!   Wiring is a v1 stub; tracked under
//!   `[TD-REGISTRY-EXTENSION-WIRING]`.
//!
//! Built-in catalog construction (~47 entries) lives under
//! `builtins/` per family.

mod builtins;
mod canonical_fn;
mod extension;
mod registry;
mod spec;

pub use canonical_fn::CanonicalFn;
pub use extension::RegistryExtension;
pub use registry::{function_registry, FunctionRegistry};
pub use spec::{
    Additivity, DimensionAxis, FnSignature, FunctionCategory, FunctionSpec, ParamType,
    ReturnTypeRule,
};
