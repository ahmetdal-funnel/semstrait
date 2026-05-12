//! `ModelBuildErrorKind` — fused kind for the loader pipeline
//! (`32 §9.6`). Composes [`ParseErrorKind`], [`ValidateErrorKind`],
//! [`CatalogsParseErrorKind`], plus loader-internal failures.

use crate::error::catalogs::CatalogsParseErrorKind;
use crate::error::parse::ParseErrorKind;
use crate::error::validate::ValidateErrorKind;
use semstrait_core::diagnostic::{Diagnose, Severity};
use std::io::ErrorKind as IoErrorKind;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ModelBuildErrorKind {
    /// The loader was driven without configuring a source. Surfaces
    /// when callers invoke `build()` on a default loader without
    /// first calling `from_yaml_str` / `from_yaml_file`.
    NoSource,

    /// `SourceFs::read(path)` failed.
    SourceIo { path: String, error: IoErrorKind },

    Parse(ParseErrorKind),
    CatalogsParse(CatalogsParseErrorKind),
    Validate(ValidateErrorKind),

    /// Per-field builder-internal error (e.g. invalid newtype payload
    /// rejected at `.build()`).
    BuilderField {
        struct_name: &'static str,
        field: &'static str,
        message: String,
    },
}

impl From<ParseErrorKind> for ModelBuildErrorKind {
    fn from(k: ParseErrorKind) -> Self {
        Self::Parse(k)
    }
}

impl From<ValidateErrorKind> for ModelBuildErrorKind {
    fn from(k: ValidateErrorKind) -> Self {
        Self::Validate(k)
    }
}

impl From<CatalogsParseErrorKind> for ModelBuildErrorKind {
    fn from(k: CatalogsParseErrorKind) -> Self {
        Self::CatalogsParse(k)
    }
}

impl Diagnose for ModelBuildErrorKind {
    fn message(&self) -> String {
        match self {
            ModelBuildErrorKind::Parse(k) => k.message(),
            ModelBuildErrorKind::CatalogsParse(k) => k.message(),
            ModelBuildErrorKind::Validate(k) => k.message(),
            ModelBuildErrorKind::NoSource => {
                "model loader was invoked without a configured source".to_string()
            }
            ModelBuildErrorKind::SourceIo { path, error } => {
                format!("failed to read `{}`: {:?}", path, error)
            }
            ModelBuildErrorKind::BuilderField {
                struct_name,
                field,
                message,
            } => {
                format!("builder error in `{}::{}`: {}", struct_name, field, message)
            }
        }
    }

    fn default_severity(&self) -> Severity {
        match self {
            ModelBuildErrorKind::Parse(k) => k.default_severity(),
            ModelBuildErrorKind::CatalogsParse(k) => k.default_severity(),
            ModelBuildErrorKind::Validate(k) => k.default_severity(),
            ModelBuildErrorKind::NoSource
            | ModelBuildErrorKind::SourceIo { .. }
            | ModelBuildErrorKind::BuilderField { .. } => Severity::Error,
        }
    }
}
