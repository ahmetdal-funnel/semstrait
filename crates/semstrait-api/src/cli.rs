//! CLI transport using clap.
//!
//! Commands:
//! - compile: Compile YAML model files to CompiledManifest
//! - explain: Show query plan, SQL, and Substrait JSON for a query
//! - validate: Validate a query request against a manifest
//! - query: Execute a query via DataFusion (feature-gated)
//! - query-duckdb: Execute a query via DuckDB (feature-gated)
//! - serve: Start the REST API server (feature-gated)

use crate::engine::SemstraitEngine;
use crate::types::RawQueryRequest;
use clap::{Parser, Subcommand};
use semstrait_manifest::{CompileSource, ManifestCompiler};
use std::path::PathBuf;
use std::sync::Arc;

/// Semstrait CLI — semantic model compiler and query engine.
#[derive(Parser, Debug)]
#[command(name = "semstrait")]
#[command(version, about = "Semantic model compiler and query engine")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Compile YAML model files into a CompiledManifest.
    Compile {
        /// Path to YAML model file.
        #[arg(short, long)]
        input: PathBuf,

        /// Output path for the compiled manifest JSON.
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Path to catalogs.yaml for catalog connections.
        #[arg(long)]
        catalogs: Option<PathBuf>,
    },

    /// Show the query plan, SQL, and Substrait JSON for a query.
    Explain {
        /// Path to YAML model file.
        #[arg(short, long)]
        model: PathBuf,

        /// Entity to query (kind or dataset name).
        #[arg(short, long)]
        from: String,

        /// Semantic names to select (auto-classified as dimensions/measures).
        #[arg(short, long, num_args = 1..)]
        select: Vec<String>,

        /// Named filters to apply.
        #[arg(long, num_args = 0..)]
        filters: Vec<String>,

        /// Output as JSON instead of text.
        #[arg(long)]
        json: bool,

        /// Path to catalogs.yaml for catalog connections.
        #[arg(long)]
        catalogs: Option<PathBuf>,
    },

    /// Validate a query request against a manifest.
    Validate {
        /// Path to YAML model file.
        #[arg(short, long)]
        model: PathBuf,

        /// Entity to query (kind or dataset name).
        #[arg(short, long)]
        from: String,

        /// Semantic names to select (auto-classified as dimensions/measures).
        #[arg(short, long, num_args = 1..)]
        select: Vec<String>,

        /// Named filters to apply.
        #[arg(long, num_args = 0..)]
        filters: Vec<String>,

        /// Path to catalogs.yaml for catalog connections.
        #[arg(long)]
        catalogs: Option<PathBuf>,
    },

    /// Execute a query against local data files via DataFusion.
    #[cfg(feature = "datafusion")]
    Query {
        /// Path to YAML model file.
        #[arg(short, long)]
        model: PathBuf,

        /// Entity to query (kind or dataset name).
        #[arg(short, long)]
        from: String,

        /// Semantic names to select (auto-classified as dimensions/measures).
        #[arg(short, long, num_args = 1..)]
        select: Vec<String>,

        /// Named filters to apply.
        #[arg(long, num_args = 0..)]
        filters: Vec<String>,

        /// Register data files: name=path pairs (e.g., orders_data=data/orders.csv).
        #[arg(short, long, num_args = 1..)]
        register: Vec<String>,
    },

    /// Execute a query against local data files via DuckDB.
    #[cfg(feature = "duckdb")]
    QueryDuckdb {
        /// Path to YAML model file.
        #[arg(short, long)]
        model: PathBuf,

        /// Entity to query (kind or dataset name).
        #[arg(short, long)]
        from: String,

        /// Semantic names to select (auto-classified as dimensions/measures).
        #[arg(short, long, num_args = 1..)]
        select: Vec<String>,

        /// Named filters to apply.
        #[arg(long, num_args = 0..)]
        filters: Vec<String>,

        /// Register data files: name=path pairs (e.g., orders_data=data/orders.csv).
        #[arg(short, long, num_args = 1..)]
        register: Vec<String>,

        /// Path to DuckDB database file (default: in-memory).
        #[arg(long)]
        db: Option<PathBuf>,
    },

    /// Execute a query against a Trino cluster.
    #[cfg(feature = "trino")]
    QueryTrino {
        /// Path to YAML model file.
        #[arg(short, long)]
        model: PathBuf,

        /// Entity to query (kind or dataset name).
        #[arg(short, long)]
        from: String,

        /// Semantic names to select (auto-classified as dimensions/measures).
        #[arg(short, long, num_args = 1..)]
        select: Vec<String>,

        /// Named filters to apply.
        #[arg(long, num_args = 0..)]
        filters: Vec<String>,

        /// Trino coordinator URL (e.g., http://trino:8080).
        #[arg(long)]
        trino_url: String,

        /// Trino catalog name.
        #[arg(long)]
        trino_catalog: String,

        /// Trino schema name.
        #[arg(long)]
        trino_schema: String,

        /// Trino user name.
        #[arg(long, default_value = "semstrait")]
        trino_user: String,

        /// Trino Bearer token for authentication.
        #[arg(long)]
        trino_token: Option<String>,
    },

    /// Start the REST API server.
    #[cfg(feature = "rest")]
    Serve {
        /// Path to YAML model file.
        #[arg(short, long)]
        model: PathBuf,

        /// Port to bind to.
        #[arg(short, long, default_value = "8080")]
        port: u16,

        /// Host to bind to.
        #[arg(long, default_value = "0.0.0.0")]
        host: String,

        /// Also start a gRPC server on this port.
        #[cfg(feature = "grpc")]
        #[arg(long)]
        grpc_port: Option<u16>,
    },
}

/// Compile a manifest from a YAML model file.
///
/// If `catalogs_path` is provided, parses `catalogs.yaml` and builds a
/// `CatalogRegistry` with concrete providers for each named catalog entry.
async fn compile_from_file(
    model: &PathBuf,
    catalogs_path: Option<&PathBuf>,
) -> Result<semstrait_manifest::CompiledManifest, Box<dyn std::error::Error>> {
    let yaml = tokio::fs::read_to_string(model).await?;
    let mut compiler = ManifestCompiler::new();

    if let Some(cat_path) = catalogs_path {
        let registry = build_catalog_registry(cat_path).await?;
        eprintln!(
            "Loaded {} catalog(s): {}",
            registry.len(),
            registry.aliases().collect::<Vec<_>>().join(", ")
        );
        compiler = compiler.with_catalog_registry(registry);
    }

    Ok(compiler
        .compile(CompileSource::Yaml(yaml))
        .await
        .map_err(|e| format!("compilation failed: {}", e))?)
}

/// Build a `CatalogRegistry` from a `catalogs.yaml` file.
///
/// Iterates each named catalog entry and constructs the appropriate
/// `CatalogProvider` based on the entry's `provider_type` and `auth` method.
async fn build_catalog_registry(
    catalogs_path: &PathBuf,
) -> Result<semstrait_catalog::CatalogRegistry, Box<dyn std::error::Error>> {
    let yaml = tokio::fs::read_to_string(catalogs_path).await?;
    let config = semstrait_model::parse_catalogs(&yaml)
        .map_err(|e| format!("failed to parse catalogs.yaml: {}", e))?;

    let mut registry = semstrait_catalog::CatalogRegistry::new();

    for (alias, entry) in &config.catalogs {
        let provider = build_catalog_provider(alias, entry).await?;
        registry.register(alias, provider);
    }

    Ok(registry)
}

/// Build a single `CatalogProvider` from a `CatalogEntry`.
async fn build_catalog_provider(
    alias: &str,
    entry: &semstrait_model::CatalogEntry,
) -> Result<Arc<dyn semstrait_catalog::CatalogProvider>, Box<dyn std::error::Error>> {
    match entry.provider_type.as_str() {
        "polaris" | "iceberg_rest" => build_iceberg_provider(alias, entry).await,
        other => Err(format!(
            "catalog '{}': unsupported provider type '{}' (supported: polaris, iceberg_rest)",
            alias, other
        )
        .into()),
    }
}

/// Build an Iceberg REST catalog provider from a `CatalogEntry`.
#[cfg(feature = "iceberg")]
async fn build_iceberg_provider(
    alias: &str,
    entry: &semstrait_model::CatalogEntry,
) -> Result<Arc<dyn semstrait_catalog::CatalogProvider>, Box<dyn std::error::Error>> {
    use semstrait_model::CatalogAuthMethod;

    match &entry.auth {
        #[cfg(feature = "aws")]
        CatalogAuthMethod::AwsSecrets {
            secret_arn,
            region,
            scope,
            aws_profile,
            aws_access_key_id,
            aws_secret_access_key,
            aws_session_token,
            ..
        } => {
            let config = semstrait_catalog::secrets::PolarisCatalogConfig {
                catalog_url: &entry.url,
                secret_arn,
                aws_region: region.as_deref(),
                warehouse: Some(&entry.name),
                realm: entry.realm.as_deref(),
                scope: scope.as_deref(),
                aws_profile: aws_profile.as_deref(),
                aws_access_key_id: aws_access_key_id.as_deref(),
                aws_secret_access_key: aws_secret_access_key.as_deref(),
                aws_session_token: aws_session_token.as_deref(),
            };
            let catalog = semstrait_catalog::secrets::build_polaris_catalog(&config)
                .await
                .map_err(|e| format!("catalog '{}': failed to build Polaris provider: {}", alias, e))?;
            Ok(Arc::new(catalog))
        }

        CatalogAuthMethod::Oauth2 {
            token_url,
            client_id,
            client_secret,
            scope,
        } => {
            let url = token_url
                .clone()
                .unwrap_or_else(|| format!("{}/v1/oauth/tokens", entry.url));
            let mut catalog = semstrait_catalog::IcebergRestCatalog::new(&entry.url)
                .with_prefix(&entry.name)
                .with_oauth2(url, client_id, client_secret, scope.clone());
            if let Some(ref realm) = entry.realm {
                catalog = catalog.with_custom_header("Polaris-Realm", realm);
            }
            Ok(Arc::new(catalog))
        }

        CatalogAuthMethod::Bearer { token } => {
            let mut catalog = semstrait_catalog::IcebergRestCatalog::new(&entry.url)
                .with_prefix(&entry.name)
                .with_bearer_token(token);
            if let Some(ref realm) = entry.realm {
                catalog = catalog.with_custom_header("Polaris-Realm", realm);
            }
            Ok(Arc::new(catalog))
        }

        #[cfg(not(feature = "aws"))]
        CatalogAuthMethod::AwsSecrets { .. } => {
            Err(format!(
                "catalog '{}': aws_secrets auth requires the 'aws' feature",
                alias
            )
            .into())
        }
    }
}

#[cfg(not(feature = "iceberg"))]
async fn build_iceberg_provider(
    alias: &str,
    _entry: &semstrait_model::CatalogEntry,
) -> Result<Arc<dyn semstrait_catalog::CatalogProvider>, Box<dyn std::error::Error>> {
    Err(format!(
        "catalog '{}': iceberg/polaris provider requires the 'iceberg' feature",
        alias
    )
    .into())
}

/// Build a RawQueryRequest from common CLI args.
fn build_raw_request(from: String, select: Vec<String>, filters: Vec<String>) -> RawQueryRequest {
    RawQueryRequest {
        model: None, // model is loaded separately
        from,
        select,
        filters,
        ..Default::default()
    }
}

/// Execute a query and print the result as pretty JSON.
#[cfg(any(feature = "datafusion", feature = "duckdb", feature = "trino"))]
async fn run_query(
    engine: &SemstraitEngine,
    raw: &RawQueryRequest,
) -> Result<(), Box<dyn std::error::Error>> {
    let result = engine.query(raw).await?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

/// Run the CLI.
pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Compile {
            input,
            output,
            catalogs,
        } => {
            let manifest = compile_from_file(&input, catalogs.as_ref()).await?;
            let json = serde_json::to_string_pretty(&manifest)?;
            match output {
                Some(path) => {
                    tokio::fs::write(&path, &json).await?;
                    eprintln!("Compiled manifest written to {}", path.display());
                }
                None => println!("{}", json),
            }
            Ok(())
        }

        Commands::Explain {
            model,
            from,
            select,
            filters,
            json,
            catalogs,
        } => {
            let manifest = compile_from_file(&model, catalogs.as_ref()).await?;
            let engine = SemstraitEngine::with_manifest(manifest);
            let raw = build_raw_request(from, select, filters);
            let result = engine.explain(&raw).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                if let Some(sql) = &result.sql {
                    println!("--- SQL ---\n{}\n", sql);
                }
                println!("--- Plan ---\n{}\n", result.plan_text);
                if let Some(substrait) = &result.substrait_json {
                    println!("--- Substrait JSON ---\n{}\n", substrait);
                }
            }
            Ok(())
        }

        Commands::Validate {
            model,
            from,
            select,
            filters,
            catalogs,
        } => {
            let manifest = compile_from_file(&model, catalogs.as_ref()).await?;
            let engine = SemstraitEngine::with_manifest(manifest);
            let raw = build_raw_request(from, select, filters);
            let result = engine.validate(&raw);
            if result.valid {
                println!("Valid");
                for w in &result.warnings {
                    eprintln!("warning: {}", w);
                }
            } else {
                eprintln!("Invalid:");
                for e in &result.errors {
                    eprintln!("  error: {}", e);
                }
                std::process::exit(1);
            }
            Ok(())
        }

        #[cfg(feature = "datafusion")]
        Commands::Query {
            model,
            from,
            select,
            filters,
            register,
        } => {
            let compiled = compile_from_file(&model).await?;

            let connector = semstrait_connectors::datafusion::DataFusionConnector::new();
            for pair in &register {
                let (name, path) = pair
                    .split_once('=')
                    .ok_or_else(|| format!("invalid --register format '{}': expected name=path", pair))?;
                connector.register_file(name, path).await
                    .map_err(|e| format!("failed to register '{}': {}", pair, e))?;
                eprintln!("Registered table '{}' from {}", name, path);
            }

            let engine = SemstraitEngine::with_connector(compiled, Arc::new(connector));
            let raw = build_raw_request(from, select, filters);
            run_query(&engine, &raw).await
        }

        #[cfg(feature = "duckdb")]
        Commands::QueryDuckdb {
            model,
            from,
            select,
            filters,
            register,
            db,
        } => {
            let compiled = compile_from_file(&model).await?;

            let connector = match db {
                Some(path) => semstrait_connectors::duckdb::DuckDbConnector::with_path(
                    &path.to_string_lossy(),
                )?,
                None => semstrait_connectors::duckdb::DuckDbConnector::new()?,
            };
            for pair in &register {
                let (name, path) = pair
                    .split_once('=')
                    .ok_or_else(|| format!("invalid --register format '{}': expected name=path", pair))?;
                connector.register_file(name, path).await
                    .map_err(|e| format!("failed to register '{}': {}", pair, e))?;
                eprintln!("Registered table '{}' from {}", name, path);
            }

            let engine = SemstraitEngine::with_connector(compiled, Arc::new(connector));
            let raw = build_raw_request(from, select, filters);
            run_query(&engine, &raw).await
        }

        #[cfg(feature = "trino")]
        Commands::QueryTrino {
            model,
            from,
            select,
            filters,
            trino_url,
            trino_catalog,
            trino_schema,
            trino_user,
            trino_token,
        } => {
            let compiled = compile_from_file(&model).await?;

            let mut connector =
                semstrait_connectors::trino::TrinoConnector::new(trino_url, trino_catalog, trino_schema)
                    .with_user(trino_user);
            if let Some(token) = trino_token {
                connector = connector.with_bearer_token(token);
            }

            let engine = SemstraitEngine::with_connector(compiled, Arc::new(connector));
            let raw = build_raw_request(from, select, filters);
            run_query(&engine, &raw).await
        }

        #[cfg(feature = "rest")]
        Commands::Serve {
            model,
            port,
            host,
            #[cfg(feature = "grpc")]
            grpc_port,
        } => {
            let yaml = tokio::fs::read_to_string(&model).await?;
            let engine = SemstraitEngine::with_manifest_yaml(&yaml).await?;
            let shared = Arc::new(engine);

            // Optionally start gRPC server in background.
            #[cfg(feature = "grpc")]
            if let Some(gport) = grpc_port {
                let grpc_engine = shared.clone();
                let grpc_addr = format!("{}:{}", host, gport);
                let addr: std::net::SocketAddr = grpc_addr.parse()
                    .map_err(|e| format!("invalid gRPC address '{}': {}", grpc_addr, e))?;
                eprintln!("gRPC listening on {}", grpc_addr);
                let grpc_svc = crate::grpc::SemstraitGrpcService::new(grpc_engine);
                tokio::spawn(async move {
                    if let Err(e) = tonic::transport::Server::builder()
                        .add_service(grpc_svc.into_server())
                        .serve(addr)
                        .await
                    {
                        eprintln!("gRPC server error: {}", e);
                    }
                });
            }

            let app = crate::rest::router(shared);
            let addr = format!("{}:{}", host, port);
            eprintln!("REST listening on http://{}", addr);
            let listener = tokio::net::TcpListener::bind(&addr).await?;
            axum::serve(listener, app).await?;
            Ok(())
        }
    }
}
