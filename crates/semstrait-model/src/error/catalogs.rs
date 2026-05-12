//! `CatalogsParseErrorKind` — `32b §5.2`.

use semstrait_core::diagnostic::{Diagnose, Severity};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CatalogsParseErrorKind {
    YamlSyntax {
        message: String,
    },
    UnsetEnvVar {
        var: String,
    },
    UnknownTopLevelKey {
        key: String,
    },
    CatalogMissingField {
        alias: String,
        field: String,
    },
    MalformedAuthMethod {
        alias: String,
        reason: String,
    },
    UnknownField {
        field: String,
        parent: String,
    },
}

impl Diagnose for CatalogsParseErrorKind {
    fn message(&self) -> String {
        use CatalogsParseErrorKind::*;
        match self {
            YamlSyntax { message } => format!("catalogs YAML syntax error: {}", message),
            UnsetEnvVar { var } => {
                format!("environment variable `{}` is not set", var)
            }
            UnknownTopLevelKey { key } => format!(
                "unknown top-level key `{}` (only `catalogs:` is recognized)",
                key
            ),
            CatalogMissingField { alias, field } => format!(
                "catalog `{}` is missing required field `{}`",
                alias, field
            ),
            MalformedAuthMethod { alias, reason } => {
                format!("catalog `{}`: malformed auth method: {}", alias, reason)
            }
            UnknownField { field, parent } => {
                format!("unknown field `{}` in `{}`", field, parent)
            }
        }
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }
}
