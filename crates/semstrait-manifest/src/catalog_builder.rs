//! Builds a `CatalogProvider` from `CatalogConnectionConfig` in the YAML model.

use std::sync::Arc;

use semstrait_catalog::CatalogProvider;
use semstrait_model::CatalogConnectionConfig;

use crate::error::CompileError;

/// Build a catalog provider from the model's catalog connection configuration.
///
/// Feature-gated: requires `iceberg` for Iceberg REST catalogs and `aws-secrets`
/// for the Polaris auth method. Returns an error if the catalog type or auth
/// method requires a feature that is not compiled in.
pub(crate) async fn build_from_config(
    config: &CatalogConnectionConfig,
) -> Result<Arc<dyn CatalogProvider>, CompileError> {
    match config.catalog_type.as_str() {
        #[cfg(feature = "iceberg")]
        "iceberg_rest" => build_iceberg(config).await,

        #[cfg(not(feature = "iceberg"))]
        "iceberg_rest" => Err(CompileError::CatalogConfig(
            "catalog type 'iceberg_rest' requires the 'iceberg' feature".to_owned(),
        )),

        other => Err(CompileError::CatalogConfig(format!(
            "unsupported catalog type: '{other}'"
        ))),
    }
}

#[cfg(feature = "iceberg")]
async fn build_iceberg(
    config: &CatalogConnectionConfig,
) -> Result<Arc<dyn CatalogProvider>, CompileError> {
    use semstrait_model::CatalogAuthConfig;

    let mut catalog = semstrait_catalog::IcebergRestCatalog::new(&config.url);

    if let Some(wh) = &config.warehouse {
        catalog = catalog.with_warehouse(wh);
    }
    if let Some(prefix) = &config.prefix {
        catalog = catalog.with_prefix(prefix);
    }

    match &config.auth {
        None | Some(CatalogAuthConfig::None) => {}
        Some(CatalogAuthConfig::Bearer { token }) => {
            catalog = catalog.with_bearer_token(token);
        }
        Some(CatalogAuthConfig::Oauth2 {
            token_url,
            client_id,
            client_secret,
            scope,
        }) => {
            catalog = catalog.with_oauth2(token_url, client_id, client_secret, scope.clone());
        }
        #[cfg(feature = "aws-secrets")]
        Some(CatalogAuthConfig::Polaris {
            secret_arn,
            region,
            realm,
            scope,
            aws_profile,
            aws_access_key_id,
            aws_secret_access_key,
            aws_session_token,
        }) => {
            let polaris_config = semstrait_catalog::secrets::PolarisCatalogConfig {
                catalog_url: &config.url,
                secret_arn,
                aws_region: region.as_deref(),
                warehouse: config.warehouse.as_deref(),
                realm: realm.as_deref(),
                scope: scope.as_deref(),
                aws_profile: aws_profile.as_deref(),
                aws_access_key_id: aws_access_key_id.as_deref(),
                aws_secret_access_key: aws_secret_access_key.as_deref(),
                aws_session_token: aws_session_token.as_deref(),
            };
            catalog = semstrait_catalog::secrets::build_polaris_catalog(&polaris_config)
                .await
                .map_err(|e| CompileError::CatalogConfig(format!("Polaris catalog error: {e}")))?;
        }
        #[cfg(not(feature = "aws-secrets"))]
        Some(CatalogAuthConfig::Polaris { .. }) => {
            return Err(CompileError::CatalogConfig(
                "auth method 'polaris' requires the 'aws-secrets' feature".to_owned(),
            ));
        }
    }

    Ok(Arc::new(catalog))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_build_unknown_type() {
        let config = CatalogConnectionConfig {
            catalog_type: "unknown".to_owned(),
            url: "https://example.com".to_owned(),
            warehouse: None,
            prefix: None,
            auth: None,
        };
        let result = build_from_config(&config).await;
        assert!(result.is_err());
        match result {
            Err(e) => assert!(e.to_string().contains("unsupported catalog type")),
            Ok(_) => panic!("expected error"),
        }
    }
}
