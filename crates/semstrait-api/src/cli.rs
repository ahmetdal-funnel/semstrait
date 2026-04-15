//! CLI transport using clap.
//!
//! Commands:
//! - compile: Compile YAML model files to CompiledManifest
//! - explain: Show query plan, SQL, and Substrait JSON for a query
//! - validate: Validate a query request against a manifest
//! - serve: Start the REST API server (feature-gated)

use crate::engine::{resolve_adapter, SemstraitEngine};
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
        #[arg(short, long)]
        catalogs: Option<PathBuf>,
    },

    /// Show the query plan and/or SQL for a query.
    Explain {
        /// Path to YAML model file.
        #[arg(short, long)]
        model: PathBuf,

        /// Entity to query. If omitted and model has exactly one entity, auto-selects it.
        #[arg(short, long)]
        from: Option<String>,

        /// Semantic names to select (auto-classified as dimensions/measures).
        #[arg(short, long, num_args = 1..)]
        select: Vec<String>,

        /// Named filters to apply.
        #[arg(long, num_args = 0..)]
        filters: Vec<String>,

        /// Output format: "plan", "sql", or omit for both.
        #[arg(short, long)]
        output: Option<String>,

        /// Output as JSON instead of text.
        #[arg(long)]
        json: bool,

        /// Path to catalogs.yaml for catalog connections.
        #[arg(short, long)]
        catalogs: Option<PathBuf>,

        /// Engine adapter: "datafusion", "ansi" (default: ansi).
        #[arg(short, long)]
        engine: Option<String>,
    },

    /// Validate a query request against a manifest.
    Validate {
        /// Path to YAML model file.
        #[arg(short, long)]
        model: PathBuf,

        /// Entity to query. If omitted and model has exactly one entity, auto-selects it.
        #[arg(short, long)]
        from: Option<String>,

        /// Semantic names to select (auto-classified as dimensions/measures).
        #[arg(short, long, num_args = 1..)]
        select: Vec<String>,

        /// Named filters to apply.
        #[arg(long, num_args = 0..)]
        filters: Vec<String>,

        /// Path to catalogs.yaml for catalog connections.
        #[arg(short, long)]
        catalogs: Option<PathBuf>,

        /// Engine adapter: "datafusion", "ansi" (default: ansi).
        #[arg(short, long)]
        engine: Option<String>,
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

        /// Engine adapter: "datafusion", "ansi" (default: ansi).
        #[arg(short, long)]
        engine: Option<String>,

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

/// Build a SemstraitEngine from a compiled manifest and optional engine name.
///
/// When `engine_name` is "datafusion", wires the DataFusion adapter's plan_builder
/// into the planner for engine-specific node construction.
fn build_engine(
    manifest: semstrait_manifest::CompiledManifest,
    engine_name: Option<&str>,
) -> Result<SemstraitEngine, Box<dyn std::error::Error>> {
    match resolve_adapter(engine_name)? {
        Some(adapter) => {
            eprintln!("Engine: {}", adapter.name());
            Ok(SemstraitEngine::with_adapter(manifest, adapter))
        }
        None => {
            eprintln!("Engine: ansi (canonical)");
            Ok(SemstraitEngine::with_manifest(manifest))
        }
    }
}

/// Build a RawQueryRequest from common CLI args.
fn build_raw_request(from: Option<String>, select: Vec<String>, filters: Vec<String>) -> RawQueryRequest {
    RawQueryRequest {
        model: None,
        from,
        select,
        filters,
        ..Default::default()
    }
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
            output,
            json,
            catalogs,
            engine: engine_name,
        } => {
            let manifest = compile_from_file(&model, catalogs.as_ref()).await?;
            let engine = build_engine(manifest, engine_name.as_deref())?;
            let raw = build_raw_request(from, select, filters);
            let result = engine.explain(&raw).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                let show_plan = output.as_deref().is_none() || output.as_deref() == Some("plan");
                let show_sql = output.as_deref().is_none() || output.as_deref() == Some("sql");
                if show_plan {
                    println!("--- Plan ---\n{}\n", result.plan_text);
                }
                if show_sql {
                    if let Some(sql) = &result.sql {
                        println!("--- SQL ---\n{}\n", sql);
                    }
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
            engine: engine_name,
        } => {
            let manifest = compile_from_file(&model, catalogs.as_ref()).await?;
            let engine = build_engine(manifest, engine_name.as_deref())?;
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

        #[cfg(feature = "rest")]
        Commands::Serve {
            model,
            port,
            host,
            engine: engine_name,
            #[cfg(feature = "grpc")]
            grpc_port,
        } => {
            let manifest = compile_from_file(&model, None).await?;
            let engine = build_engine(manifest, engine_name.as_deref())?;
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
