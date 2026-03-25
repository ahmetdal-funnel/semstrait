//! Adapter error types.

/// Errors that can occur during plan adaptation.
#[derive(Debug, thiserror::Error)]
pub enum AdaptError {
    /// SQL emission failed.
    #[error("SQL emission failed: {0}")]
    SqlEmission(String),

    /// Substrait serialization failed.
    #[error("Substrait serialization failed: {0}")]
    SubstraitSerialization(String),

    /// The plan uses a feature not supported by the target engine.
    #[error("unsupported plan feature: {0}")]
    UnsupportedFeature(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_sql_emission() {
        let err = AdaptError::SqlEmission("column not found".to_string());
        assert_eq!(err.to_string(), "SQL emission failed: column not found");
    }

    #[test]
    fn test_error_display_substrait() {
        let err = AdaptError::SubstraitSerialization("encode failed".to_string());
        assert_eq!(
            err.to_string(),
            "Substrait serialization failed: encode failed"
        );
    }

    #[test]
    fn test_error_display_unsupported() {
        let err = AdaptError::UnsupportedFeature("FULL OUTER JOIN".to_string());
        assert_eq!(
            err.to_string(),
            "unsupported plan feature: FULL OUTER JOIN"
        );
    }

    #[test]
    fn test_error_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<AdaptError>();
        assert_sync::<AdaptError>();
    }
}
