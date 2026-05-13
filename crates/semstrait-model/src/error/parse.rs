//! `ParseErrorKind` — `32 §9.2`.

use semstrait_core::diagnostic::{Diagnose, Severity};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ParseErrorKind {
    // — YAML surface —
    YamlSyntax {
        message: String,
    },
    UnsetEnvVar {
        var: String,
    },
    UnknownField {
        field: String,
        parent: String,
    },

    // — Structural rules (SR-*) —
    InvalidIdentifier {
        raw: String,
        reason: String,
    },

    // — Entity-level (SR-E-*) — fired during parse —
    RelationshipMissingCardinality {
        relationship: String,
    },
    MeasureMissingAgg {
        carrier: String,
        name: String,
    },
    SemanticsMissingDataType {
        carrier: String,
        name: String,
    },
}

impl Diagnose for ParseErrorKind {
    fn message(&self) -> String {
        use ParseErrorKind::*;
        match self {
            YamlSyntax { message } => format!("YAML syntax error: {}", message),
            UnsetEnvVar { var } => {
                format!("environment variable `{}` is not set", var)
            }
            UnknownField { field, parent } => {
                format!("unknown field `{}` in `{}`", field, parent)
            }
            InvalidIdentifier { raw, reason } => {
                format!("invalid identifier `{}`: {}", raw, reason)
            }
            RelationshipMissingCardinality { relationship } => format!(
                "relationship `{}` is missing required `cardinality:` (SR-E-4)",
                relationship
            ),
            MeasureMissingAgg { carrier, name } => format!(
                "{} `{}` is missing required `agg:` (SR-E-9)",
                carrier, name
            ),
            SemanticsMissingDataType { carrier, name } => format!(
                "{} `{}` is missing required `data_type:` (SR-E-10)",
                carrier, name
            ),
        }
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }
}
