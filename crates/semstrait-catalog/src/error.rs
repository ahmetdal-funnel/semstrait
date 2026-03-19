//! Catalog error types.

use thiserror::Error;

/// Errors that can occur during catalog operations.
#[derive(Debug, Error)]
pub enum CatalogError {
    /// Catalog is not available or not configured
    #[error("catalog not available: {0}")]
    NotAvailable(String),

    /// Table was not found in the catalog
    #[error("table not found: {0}")]
    TableNotFound(String),

    /// Namespace/schema/database was not found
    #[error("namespace not found: {0}")]
    NamespaceNotFound(String),

    /// Connection error (network, timeout, authentication, etc.)
    #[error("connection error: {0}")]
    ConnectionError(String),

    /// Internal catalog error
    #[error("internal error: {0}")]
    Internal(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catalog_error_display() {
        let err = CatalogError::TableNotFound("sales.orders".to_string());
        assert_eq!(format!("{}", err), "table not found: sales.orders");

        let err = CatalogError::NamespaceNotFound("sales".to_string());
        assert_eq!(format!("{}", err), "namespace not found: sales");

        let err = CatalogError::ConnectionError("timeout".to_string());
        assert_eq!(format!("{}", err), "connection error: timeout");
    }
}
