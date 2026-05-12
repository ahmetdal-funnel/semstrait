//! Newtype identifiers used across the model author surface.
//!
//! - [`SemanticsName`] — Dimension / Measure / Metric name (root-pool
//!   key or inline declaration name).
//! - [`DataKindName`] — Dataset / Grainset / Unionset / Joinset name.
//! - [`FilterName`] — DataKindFilter / AggregationFilter name.
//!
//! All three are transparent newtypes around `String`. Identifier
//! grammar enforcement (per `00 §4.1` / `11 §4`) lands at validate time
//! through `ParseErrorKind::InvalidIdentifier`; the types themselves do
//! NOT validate at construction so `parse` can collect every offending
//! identifier rather than fail-fast on the first one.

use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! str_newtype {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(s: impl Into<String>) -> Self {
                Self(s.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn into_string(self) -> String {
                self.0
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_string())
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

str_newtype!(
    SemanticsName,
    "Identifier for a `Dimension`, `Measure`, or `Metric` (root-pool key or inline declaration name)."
);
str_newtype!(
    DataKindName,
    "Identifier for a `Dataset`, `Grainset`, `Unionset`, or `Joinset` (top-level map key)."
);
str_newtype!(
    FilterName,
    "Identifier for a `DataKindFilter` or `AggregationFilter`."
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newtype_roundtrip_serde() {
        let n: SemanticsName = "revenue".into();
        let json = serde_json::to_string(&n).unwrap();
        assert_eq!(json, "\"revenue\"");
        let back: SemanticsName = serde_json::from_str(&json).unwrap();
        assert_eq!(back, n);
    }

    #[test]
    fn ordering_alphabetical() {
        let mut v = [
            DataKindName::from("zeta"),
            DataKindName::from("alpha"),
            DataKindName::from("mu"),
        ];
        v.sort();
        let names: Vec<&str> = v.iter().map(|n| n.as_str()).collect();
        assert_eq!(names, vec!["alpha", "mu", "zeta"]);
    }
}
