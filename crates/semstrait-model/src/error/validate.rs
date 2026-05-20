//! `ValidateErrorKind` — `32 §9.5` + entity-level SR-E-* per `18 §11`.

use semstrait_core::diagnostic::{Diagnose, Location, Severity};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ValidateErrorKind {
    // — Composition shape (SR-10) —
    ComplexDataKindInsufficientChildren {
        parent: String,
        child_count: usize,
    },

    // — Empty model —
    EmptyModel,

    // — Cross-source / single-file dup detection (SR-3, D-10) —
    DuplicateDataKindName {
        name: String,
        occurrences: Vec<Location>,
    },
    DuplicateSharedSemanticsName {
        carrier: String,
        name: String,
        occurrences: Vec<Location>,
    },

    // — Entity-level invariants (SR-E-*) —
    /// SR-E-2
    SemanticsRefMissingExpr {
        carrier: String,
        name: String,
    },
    /// SR-E-3
    OrphanSharedSemantics {
        carrier: String,
        name: String,
    },
    /// SR-E-5
    RelationshipDanglingEndpoint {
        relationship: String,
        side: String,
        endpoint: String,
    },
    /// SR-E-6
    TemporalLeafMissingGrain {
        data_kind: String,
    },
    /// SR-E-7
    TemporalGrainOnComplex {
        data_kind: String,
    },
    /// SR-E-8
    GrainsetChildMissingGrain {
        grainset: String,
        child: String,
    },
    /// SR-E-11
    WrongFilterError {
        name: String,
        expected: String,
        actual: String,
    },
    /// SR-E-13
    RelationshipSymmetricCardinalityIncomplete {
        relationship: String,
        missing: String,
    },
    /// SR-E-14
    RelationshipManyToManyCrossFilterDirectional {
        relationship: String,
    },

    // — Shadowing warning (`18 §1.5`) —
    SemanticsShadowRootPool {
        carrier: String,
        name: String,
    },

    /// Reference-graph cycle among declared semantic entities.
    /// E.g. `metric A` references `metric B` which references `metric A`.
    /// Members are listed in the order they appear in the cycle starting
    /// at the lex-smallest member, for stable diagnostics per `00 §9` I4.
    /// Per `19 §3.5`'s cycle-detection algorithm; lifted to validate-time
    /// because cycles can be detected without binding/source resolution.
    CyclicSemanticsReference {
        carrier: String,
        cycle: Vec<String>,
    },
}

impl Diagnose for ValidateErrorKind {
    fn message(&self) -> String {
        use ValidateErrorKind::*;
        match self {
            ComplexDataKindInsufficientChildren {
                parent,
                child_count,
            } => format!(
                "complex data kind `{}` has only {} child(ren); SR-10 requires at least 2",
                parent, child_count
            ),
            EmptyModel => "model has no data kinds (empty model)".to_string(),
            DuplicateDataKindName { name, occurrences } => format!(
                "duplicate data-kind name `{}` ({} occurrences)",
                name,
                occurrences.len()
            ),
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
            SemanticsRefMissingExpr { carrier, name } => format!(
                "ref site for {} `{}` and root-pool entry both lack `expr:` (SR-E-2)",
                carrier, name
            ),
            OrphanSharedSemantics { carrier, name } => format!(
                "{} `{}` is never bound to a Dataset (SR-E-3)",
                carrier, name
            ),
            RelationshipDanglingEndpoint {
                relationship,
                side,
                endpoint,
            } => format!(
                "relationship `{}` references unknown {} endpoint `{}` (SR-E-5)",
                relationship, side, endpoint
            ),
            TemporalLeafMissingGrain { data_kind } => format!(
                "leaf `{}` has `temporal:` but is missing `grain:` (SR-E-6)",
                data_kind
            ),
            TemporalGrainOnComplex { data_kind } => format!(
                "complex data kind `{}` may not author `temporal.grain:` (SR-E-7)",
                data_kind
            ),
            GrainsetChildMissingGrain { grainset, child } => format!(
                "grainset child `{}.{}` must author its own `temporal.grain:` (SR-E-8)",
                grainset, child
            ),
            WrongFilterError {
                name,
                expected,
                actual,
            } => format!(
                "filter `{}`: expected {}, got {} (SR-E-11)",
                name, expected, actual
            ),
            RelationshipSymmetricCardinalityIncomplete {
                relationship,
                missing,
            } => format!(
                "relationship `{}` has symmetric cardinality but is missing `{}:` (SR-E-13)",
                relationship, missing
            ),
            RelationshipManyToManyCrossFilterDirectional { relationship } => format!(
                "relationship `{}`: many-to-many `cross_filter:` may not be Left/Right (SR-E-14)",
                relationship
            ),
            SemanticsShadowRootPool { carrier, name } => format!(
                "{} `{}` shadows root-pool entry; use `ref:` + override instead",
                carrier, name
            ),
            CyclicSemanticsReference { carrier, cycle } => format!(
                "cycle in `{}` references: {}",
                carrier,
                cycle.join(" → ")
            ),
        }
    }

    fn default_severity(&self) -> Severity {
        match self {
            // Shadowing is the only warning-class variant in v1.
            ValidateErrorKind::SemanticsShadowRootPool { .. } => Severity::Warning,
            _ => Severity::Error,
        }
    }
}
