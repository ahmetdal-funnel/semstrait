//! Reference-YAML round-trip — `32 §9.1`, `§9.4`, and the W5
//! acceptance criterion in the spec implementation plan.
//!
//! Asserts that:
//!  * `schemas/reference.yaml` validates against
//!    `schemas/semantic_model.schema.json`, parses, and validates clean
//!    (no `Severity::Error`).
//!  * `schemas/catalogs_reference.yaml` validates against
//!    `schemas/catalogs.schema.json` and parses clean once the env
//!    vars it references are set.

use semstrait_model::{parse, parse_catalogs};

const REFERENCE_YAML: &str = include_str!("../schemas/reference.yaml");
const CATALOGS_YAML: &str = include_str!("../schemas/catalogs_reference.yaml");

const MODEL_SCHEMA: &str = include_str!("../schemas/semantic_model.schema.json");
const CATALOGS_SCHEMA: &str = include_str!("../schemas/catalogs.schema.json");

/// Parse YAML into a `serde_json::Value` so the JSON-Schema validator
/// can consume it directly. Spec-faithful — the wire format is YAML,
/// the schema is authored against the equivalent JSON projection.
fn yaml_to_json(yaml: &str) -> serde_json::Value {
    serde_yaml::from_str(yaml).expect("yaml must parse as a json-compatible value")
}

fn validate_against_schema(schema_text: &str, instance: &serde_json::Value) {
    let schema: serde_json::Value =
        serde_json::from_str(schema_text).expect("schema must be valid JSON");
    let validator = jsonschema::validator_for(&schema).expect("schema must compile");
    let messages: Vec<String> = validator
        .iter_errors(instance)
        .map(|err| format!("schema error: {err} (at {})", err.instance_path))
        .collect();
    if !messages.is_empty() {
        for m in &messages {
            eprintln!("{m}");
        }
        panic!("instance does not validate against schema ({} errors)", messages.len());
    }
}

#[test]
fn reference_yaml_validates_against_json_schema() {
    let instance = yaml_to_json(REFERENCE_YAML);
    validate_against_schema(MODEL_SCHEMA, &instance);
}

#[test]
fn reference_yaml_parses_and_validates_clean() {
    // Post-P4: parse returns a builder; `.build()` performs uniform
    // dedup + validate and emits any warnings.
    let builder = match parse(REFERENCE_YAML) {
        Ok(b) => b,
        Err(diags) => {
            for d in &diags {
                eprintln!("parse: {:?}", d);
            }
            panic!("reference.yaml must parse clean");
        }
    };

    match builder.build() {
        Ok((_model, warnings)) => {
            assert!(
                warnings.is_empty(),
                "no warnings expected, got {:?}",
                warnings
            );
        }
        Err(diags) => {
            for d in &diags {
                eprintln!("build: {:?}", d);
            }
            panic!("reference.yaml must validate clean");
        }
    }
}

#[test]
fn catalogs_reference_yaml_validates_against_json_schema() {
    // Schema validation is structural — `${VAR}` placeholders are
    // strings either way, so substitution is unnecessary here. The
    // companion test below covers the substitution path.
    let instance = yaml_to_json(CATALOGS_YAML);
    validate_against_schema(CATALOGS_SCHEMA, &instance);
}

#[test]
fn catalogs_reference_yaml_parses_clean_with_env() {
    std::env::set_var("POLARIS_CLIENT_SECRET", "test-secret");
    std::env::set_var("ICEBERG_DEV_TOKEN", "test-token");

    let (cfg, warnings) = parse_catalogs(CATALOGS_YAML).expect("catalogs reference must parse");
    assert!(warnings.is_empty(), "no warnings expected");
    assert_eq!(cfg.catalogs.len(), 3, "three catalog entries expected");
    assert!(cfg.catalogs.contains_key("polaris_prod"));
    assert!(cfg.catalogs.contains_key("iceberg_dev"));
    assert!(cfg.catalogs.contains_key("unity_prod"));

    std::env::remove_var("POLARIS_CLIENT_SECRET");
    std::env::remove_var("ICEBERG_DEV_TOKEN");
}
