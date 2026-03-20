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

        /// Iceberg REST catalog URL for glob expansion.
        #[cfg(feature = "iceberg")]
        #[arg(long)]
        catalog_url: Option<String>,

        /// Iceberg catalog warehouse name.
        #[cfg(feature = "iceberg")]
        #[arg(long)]
        catalog_warehouse: Option<String>,

        /// Iceberg catalog Bearer token.
        #[cfg(feature = "iceberg")]
        #[arg(long)]
        catalog_token: Option<String>,
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

        /// Iceberg REST catalog URL for glob expansion.
        #[cfg(feature = "iceberg")]
        #[arg(long)]
        catalog_url: Option<String>,

        /// Iceberg catalog warehouse name.
        #[cfg(feature = "iceberg")]
        #[arg(long)]
        catalog_warehouse: Option<String>,

        /// Iceberg catalog Bearer token.
        #[cfg(feature = "iceberg")]
        #[arg(long)]
        catalog_token: Option<String>,
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

        /// Iceberg REST catalog URL for glob expansion.
        #[cfg(feature = "iceberg")]
        #[arg(long)]
        catalog_url: Option<String>,

        /// Iceberg catalog warehouse name.
        #[cfg(feature = "iceberg")]
        #[arg(long)]
        catalog_warehouse: Option<String>,

        /// Iceberg catalog Bearer token.
        #[cfg(feature = "iceberg")]
        #[arg(long)]
        catalog_token: Option<String>,
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

        /// Iceberg REST catalog URL for glob expansion.
        #[cfg(feature = "iceberg")]
        #[arg(long)]
        catalog_url: Option<String>,

        /// Iceberg catalog warehouse name.
        #[cfg(feature = "iceberg")]
        #[arg(long)]
        catalog_warehouse: Option<String>,

        /// Iceberg catalog Bearer token.
        #[cfg(feature = "iceberg")]
        #[arg(long)]
        catalog_token: Option<String>,
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

/// Build a ManifestCompiler, optionally wiring in an Iceberg catalog.
#[cfg(feature = "iceberg")]
fn build_compiler(
    catalog_url: Option<String>,
    catalog_warehouse: Option<String>,
    catalog_token: Option<String>,
) -> ManifestCompiler {
    let mut compiler = ManifestCompiler::new();
    if let Some(url) = catalog_url {
        let mut catalog = semstrait_catalog::IcebergRestCatalog::new(url);
        if let Some(wh) = catalog_warehouse {
            catalog = catalog.with_warehouse(wh);
        }
        if let Some(token) = catalog_token {
            catalog = catalog.with_bearer_token(token);
        }
        compiler = compiler.with_catalog(Arc::new(catalog));
    }
    compiler
}

#[cfg(not(feature = "iceberg"))]
fn build_compiler() -> ManifestCompiler {
    ManifestCompiler::new()
}

/// Compile a manifest from a YAML model file, optionally using an Iceberg catalog.
#[cfg(feature = "iceberg")]
async fn compile_from_file(
    model: &PathBuf,
    catalog_url: Option<String>,
    catalog_warehouse: Option<String>,
    catalog_token: Option<String>,
) -> Result<semstrait_manifest::CompiledManifest, Box<dyn std::error::Error>> {
    let yaml = tokio::fs::read_to_string(model).await?;
    let compiler = build_compiler(catalog_url, catalog_warehouse, catalog_token);
    Ok(compiler
        .compile(CompileSource::Yaml(yaml))
        .await
        .map_err(|e| format!("compilation failed: {}", e))?)
}

#[cfg(not(feature = "iceberg"))]
async fn compile_from_file(
    model: &PathBuf,
) -> Result<semstrait_manifest::CompiledManifest, Box<dyn std::error::Error>> {
    let yaml = tokio::fs::read_to_string(model).await?;
    let compiler = build_compiler();
    Ok(compiler
        .compile(CompileSource::Yaml(yaml))
        .await
        .map_err(|e| format!("compilation failed: {}", e))?)
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
            #[cfg(feature = "iceberg")]
            catalog_url,
            #[cfg(feature = "iceberg")]
            catalog_warehouse,
            #[cfg(feature = "iceberg")]
            catalog_token,
        } => {
            #[cfg(feature = "iceberg")]
            let manifest = compile_from_file(&input, catalog_url, catalog_warehouse, catalog_token).await?;
            #[cfg(not(feature = "iceberg"))]
            let manifest = compile_from_file(&input).await?;
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
        } => {
            let yaml = tokio::fs::read_to_string(&model).await?;
            let engine = SemstraitEngine::with_manifest_yaml(&yaml).await?;
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
        } => {
            let yaml = tokio::fs::read_to_string(&model).await?;
            let engine = SemstraitEngine::with_manifest_yaml(&yaml).await?;
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
            #[cfg(feature = "iceberg")]
            catalog_url,
            #[cfg(feature = "iceberg")]
            catalog_warehouse,
            #[cfg(feature = "iceberg")]
            catalog_token,
        } => {
            #[cfg(feature = "iceberg")]
            let compiled = compile_from_file(&model, catalog_url, catalog_warehouse, catalog_token).await?;
            #[cfg(not(feature = "iceberg"))]
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
            #[cfg(feature = "iceberg")]
            catalog_url,
            #[cfg(feature = "iceberg")]
            catalog_warehouse,
            #[cfg(feature = "iceberg")]
            catalog_token,
        } => {
            #[cfg(feature = "iceberg")]
            let compiled = compile_from_file(&model, catalog_url, catalog_warehouse, catalog_token).await?;
            #[cfg(not(feature = "iceberg"))]
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
            #[cfg(feature = "iceberg")]
            catalog_url,
            #[cfg(feature = "iceberg")]
            catalog_warehouse,
            #[cfg(feature = "iceberg")]
            catalog_token,
        } => {
            #[cfg(feature = "iceberg")]
            let compiled = compile_from_file(&model, catalog_url, catalog_warehouse, catalog_token).await?;
            #[cfg(not(feature = "iceberg"))]
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
                eprintln!("gRPC listening on {}", grpc_addr);
                let grpc_svc = crate::grpc::SemstraitGrpcService::new(grpc_engine);
                tokio::spawn(async move {
                    let addr = grpc_addr.parse().expect("invalid gRPC address");
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
