//! Plan artifact types — the output of the adapter layer.

use prost::Message;

/// Engine-ready plan artifact.
///
/// Produced by `EngineAdapter::adapt()`.
/// - `Sql` for engines that consume SQL strings (DuckDB, Spark).
/// - `Substrait` for engines that consume Substrait plans natively (DataFusion).
#[derive(Clone)]
pub enum PlanArtifact {
    /// Dialect-specific SQL string.
    Sql(String),
    /// Substrait plan proto object.
    /// Not stored as bytes or JSON — those are produced on demand via methods.
    Substrait(Box<substrait::proto::Plan>),
}

impl PlanArtifact {
    /// Serialize the Substrait variant to pretty JSON. Returns `None` for `Sql` variant.
    pub fn to_json(&self) -> Option<String> {
        match self {
            PlanArtifact::Substrait(plan) => serde_json::to_string_pretty(plan.as_ref()).ok(),
            PlanArtifact::Sql(_) => None,
        }
    }

    /// Serialize the Substrait variant to protobuf bytes. Returns `None` for `Sql` variant.
    pub fn to_bytes(&self) -> Option<Vec<u8>> {
        match self {
            PlanArtifact::Substrait(plan) => {
                let mut buf = Vec::new();
                plan.encode(&mut buf).ok()?;
                Some(buf)
            }
            PlanArtifact::Sql(_) => None,
        }
    }

    /// Get SQL string if this is a `Sql` artifact.
    pub fn as_sql(&self) -> Option<&str> {
        match self {
            PlanArtifact::Sql(sql) => Some(sql),
            PlanArtifact::Substrait(_) => None,
        }
    }

    /// Get Substrait plan reference if this is a `Substrait` artifact.
    pub fn as_substrait(&self) -> Option<&substrait::proto::Plan> {
        match self {
            PlanArtifact::Substrait(plan) => Some(plan),
            PlanArtifact::Sql(_) => None,
        }
    }

    /// Returns `true` if this artifact contains SQL.
    pub fn is_sql(&self) -> bool {
        matches!(self, PlanArtifact::Sql(_))
    }

    /// Returns `true` if this artifact contains a Substrait plan.
    pub fn is_substrait(&self) -> bool {
        matches!(self, PlanArtifact::Substrait(_))
    }
}

impl std::fmt::Debug for PlanArtifact {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sql(sql) => f
                .debug_tuple("Sql")
                .field(&format!("({} chars)", sql.len()))
                .finish(),
            Self::Substrait(_) => write!(f, "Substrait(<plan>)"),
        }
    }
}

impl std::fmt::Display for PlanArtifact {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sql(sql) => write!(f, "{}", sql),
            Self::Substrait(plan) => match serde_json::to_string_pretty(plan.as_ref()) {
                Ok(json) => write!(f, "{}", json),
                Err(_) => write!(f, "Substrait(<serialization failed>)"),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;

    fn make_test_plan() -> substrait::proto::Plan {
        substrait::proto::Plan {
            version: Some(substrait::proto::Version {
                major_number: 0,
                minor_number: 62,
                patch_number: 0,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn test_sql_artifact_accessors() {
        let artifact = PlanArtifact::Sql("SELECT 1".into());

        assert_eq!(artifact.as_sql(), Some("SELECT 1"));
        assert!(artifact.as_substrait().is_none());
        assert!(artifact.to_json().is_none());
        assert!(artifact.to_bytes().is_none());
        assert!(artifact.is_sql());
        assert!(!artifact.is_substrait());
    }

    #[test]
    fn test_substrait_artifact_accessors() {
        let plan = make_test_plan();
        let artifact = PlanArtifact::Substrait(Box::new(plan));

        assert!(artifact.as_sql().is_none());

        let substrait_ref = artifact.as_substrait().unwrap();
        let version = substrait_ref.version.as_ref().unwrap();
        assert_eq!(version.major_number, 0);
        assert_eq!(version.minor_number, 62);
        assert_eq!(version.patch_number, 0);

        let json = artifact.to_json().unwrap();
        // protobuf JSON serialization uses camelCase; zero-valued fields may be omitted
        assert!(
            json.contains("minorNumber") || json.contains("minor_number"),
            "JSON should contain version field: {}",
            json
        );

        let bytes = artifact.to_bytes().unwrap();
        assert!(!bytes.is_empty());

        assert!(!artifact.is_sql());
        assert!(artifact.is_substrait());
    }

    #[test]
    fn test_substrait_roundtrip() {
        let plan = make_test_plan();
        let artifact = PlanArtifact::Substrait(Box::new(plan));

        let bytes = artifact.to_bytes().unwrap();
        let decoded = substrait::proto::Plan::decode(bytes.as_slice()).unwrap();

        let version = decoded.version.as_ref().unwrap();
        assert_eq!(version.major_number, 0);
        assert_eq!(version.minor_number, 62);
        assert_eq!(version.patch_number, 0);
    }

    #[test]
    fn test_debug_format() {
        let sql_artifact = PlanArtifact::Sql("SELECT 1".into());
        let debug_sql = format!("{:?}", sql_artifact);
        assert!(debug_sql.contains("Sql"), "Debug should contain 'Sql'");
        assert!(
            debug_sql.contains("8 chars"),
            "Debug should contain char count"
        );

        let substrait_artifact = PlanArtifact::Substrait(Box::new(make_test_plan()));
        let debug_substrait = format!("{:?}", substrait_artifact);
        assert!(
            debug_substrait.contains("Substrait"),
            "Debug should contain 'Substrait'"
        );
    }

    #[test]
    fn test_display_format() {
        let sql_artifact = PlanArtifact::Sql("SELECT 1".into());
        let display_sql = format!("{}", sql_artifact);
        assert_eq!(display_sql, "SELECT 1");

        let substrait_artifact = PlanArtifact::Substrait(Box::new(make_test_plan()));
        let display_substrait = format!("{}", substrait_artifact);
        // Display for Substrait should produce JSON
        assert!(
            display_substrait.contains('{'),
            "Display should produce JSON"
        );
    }

    #[test]
    fn test_clone() {
        let sql_artifact = PlanArtifact::Sql("SELECT 1".into());
        let sql_clone = sql_artifact.clone();
        assert_eq!(sql_clone.as_sql(), Some("SELECT 1"));

        let substrait_artifact = PlanArtifact::Substrait(Box::new(make_test_plan()));
        let substrait_clone = substrait_artifact.clone();
        assert!(substrait_clone.as_substrait().is_some());
        let version = substrait_clone
            .as_substrait()
            .unwrap()
            .version
            .as_ref()
            .unwrap();
        assert_eq!(version.minor_number, 62);
    }
}
