//! `ParseErrorKind` — `32 §9.2`.

use semstrait_core::diagnostic::{Diagnose, Location, Severity};

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
    MalformedRoot {
        reason: String,
    },
    UnknownTopLevelBlock {
        block: String,
    },
    UnknownField {
        field: String,
        parent: String,
    },

    // — Structural rules (SR-*) —
    DuplicateDataKindName {
        name: String,
        occurrences: Vec<Location>,
    },
    NestedDataKindCarriesInterface {
        parent: String,
        nested: String,
        offending_field: String,
    },
    IllegalSelfNesting {
        parent_variant: String,
        nested_variant: String,
    },
    InvalidIdentifier {
        raw: String,
        reason: String,
    },

    // — Shared-pool surface —
    DuplicateSharedSemanticsName {
        carrier: String,
        name: String,
        occurrences: Vec<Location>,
    },

    // — Semantic-mapping surface —
    MalformedSemanticMappingValue {
        data_kind: String,
        semantic_name: String,
        reason: String,
    },

    // — Extras —
    MalformedCatalogRef {
        raw: String,
        reason: String,
    },
    MalformedTemporalBlock {
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
            MalformedRoot { reason } => format!("malformed model root: {}", reason),
            UnknownTopLevelBlock { block } => {
                format!("unknown top-level block `{}` (only `semantic_model:` is recognized)", block)
            }
            UnknownField { field, parent } => {
                format!("unknown field `{}` in `{}`", field, parent)
            }
            DuplicateDataKindName { name, occurrences } => format!(
                "duplicate data-kind name `{}` ({} occurrences)",
                name,
                occurrences.len()
            ),
            NestedDataKindCarriesInterface {
                parent,
                nested,
                offending_field,
            } => format!(
                "nested data kind `{}` inside `{}` cannot carry `{}` (Public-only field)",
                nested, parent, offending_field
            ),
            IllegalSelfNesting {
                parent_variant,
                nested_variant,
            } => format!(
                "illegal self-nesting: `{}` cannot contain a nested `{}`",
                parent_variant, nested_variant
            ),
            InvalidIdentifier { raw, reason } => {
                format!("invalid identifier `{}`: {}", raw, reason)
            }
            DuplicateSharedSemanticsName {
                carrier,
                name,
                occurrences,
            } => format!(
                "duplicate `{}` entry `{}` ({} occurrences)",
                carrier,
                name,
                occurrences.len()
            ),
            MalformedSemanticMappingValue {
                data_kind,
                semantic_name,
                reason,
            } => format!(
                "malformed semantic_mapping value for `{}` on `{}`: {}",
                semantic_name, data_kind, reason
            ),
            MalformedCatalogRef { raw, reason } => {
                format!("malformed catalog reference `{}`: {}", raw, reason)
            }
            MalformedTemporalBlock { reason } => {
                format!("malformed temporal block: {}", reason)
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
