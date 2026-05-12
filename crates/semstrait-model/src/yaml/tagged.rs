//! Single-key tagged-map helpers.
//!
//! `serde_yaml` 0.9 has a long-standing bug where an externally-tagged
//! Rust enum cannot be round-tripped through [`serde_yaml::Value`]: when
//! the deserializer is asked to materialise such an enum from a
//! `Value::Mapping`, it produces "invalid type: map, expected a
//! `Value::Tagged` enum" because it interprets externally-tagged enums
//! as YAML `!Tag value` syntax instead of the standard `{tag: body}`
//! single-key map form authors actually write. Tracked upstream — see
//! `dtolnay/serde-yaml#370` and adjacent issues.
//!
//! Every site in `semstrait-model` that currently authors a Rust enum
//! variant with a body via the single-key map form goes through this
//! module: `DimensionType`, `BucketBound`, `LiteralValue` (mapping
//! payload), `AdditivityType`, and `MetadataSource`. Each impl hand-
//! parses the single-key map, then delegates body deserialization to
//! [`serde_yaml::from_value`] for the variant body type — which works
//! correctly because the body is a struct/scalar, not an enum.

use serde::de::Error as DeError;
use serde_yaml::Value;

/// Decompose a YAML mapping that carries an externally-tagged enum into
/// `(tag, body)`. Errors when the map has zero or many entries, or when
/// the tag is not a string. Caller dispatches on `tag`.
pub(crate) fn single_key_map<E: DeError>(
    value: Value,
    type_name: &str,
) -> Result<(String, Value), E> {
    let map = match value {
        Value::Mapping(m) => m,
        other => {
            return Err(E::custom(format!(
                "{type_name}: expected single-key tagged map, got {other:?}"
            )));
        }
    };
    if map.len() != 1 {
        return Err(E::custom(format!(
            "{type_name}: expected single-key tagged map (e.g. `{{ tag: body }}`), got {} keys",
            map.len()
        )));
    }
    let (k, v) = map.into_iter().next().unwrap();
    let tag = k.as_str().ok_or_else(|| {
        E::custom(format!("{type_name}: variant tag must be a string"))
    })?;
    Ok((tag.to_string(), v))
}
