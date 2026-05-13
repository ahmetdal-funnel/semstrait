//! `AiContext` — LLM/agent-facing hint surface (`18 §8`).

use bon::Builder;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// LLM / agent-facing hint surface attached to root-level `SemanticModel`,
/// top-level data kinds, and individual Semantics. Never authored on
/// structural scaffolding (Nested forms, `Extras` blocks, `Relationship`
/// itself per `18 §8`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Builder)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
#[builder(start_fn = builder, finish_fn = build)]
pub struct AiContext {
    /// Synonyms the LLM may use to refer to the annotated entity. Each
    /// key is a logical alias; its value is one or more canonical forms
    /// the model exposes.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[builder(default)]
    pub synonyms: BTreeMap<String, Vec<String>>,

    /// A plain-language description the LLM may surface to the user.
    /// Duplicates the carrier's `description:` with more narrative
    /// freedom.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub description: Option<String>,

    /// Example queries or phrasings the LLM may emit.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[builder(default)]
    pub examples: Vec<String>,

    /// Unit of measurement for numeric Semantics (e.g. `"usd"`,
    /// `"percent"`, `"events_per_minute"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[builder(into)]
    pub unit: Option<String>,
}

impl AiContext {
    pub fn is_empty(&self) -> bool {
        self.synonyms.is_empty()
            && self.description.is_none()
            && self.examples.is_empty()
            && self.unit.is_none()
    }
}
