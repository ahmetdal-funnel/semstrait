//! CLI transport using clap.
//!
//! Commands:
//! - compile: Compile YAML model files to CompiledManifest
//! - explain: Show query plan, SQL, and Substrait JSON for a query
//! - validate: Validate a query request against a manifest
//! - query: Execute a query via DataFusion (feature-gated)
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
    },

    /// Show the query plan, SQL, and Substrait JSON for a query.
    Explain {
        /// Path to YAML manifest file.
        #[arg(short, long)]
        manifest: PathBuf,

        /// Kind name to query.
        #[arg(short, long)]
        kind: String,

        /// Dimensions to include.
        #[arg(short, long, num_args = 1..)]
        dimensions: Vec<String>,

        /// Measures to include.
        #[arg(short = 'e', long, num_args = 1..)]
        measures: Vec<String>,

        /// Output as JSON instead of text.
        #[arg(long)]
        json: bool,
    },

    /// Validate a query request against a manifest.
    Validate {
        /// Path to YAML manifest file.
        #[arg(short, long)]
        manifest: PathBuf,

        /// Kind name to query.
        #[arg(short, long)]
        kind: String,

        /// Dimensions to include.
        #[arg(short, long, num_args = 1..)]
        dimensions: Vec<String>,

        /// Measures to include.
        #[arg(short = 'e', long, num_args = 1..)]
        measures: Vec<String>,
    },

    /// Execute a query against local data files via DataFusion.
    #[cfg(feature = "datafusion")]
    Query {
        /// Path to YAML manifest file.
        #[arg(short, long)]
        manifest: PathBuf,

        /// Kind name to query.
        #[arg(short, long)]
        kind: String,

        /// Dimensions to include.
        #[arg(short, long, num_args = 1..)]
        dimensions: Vec<String>,

        /// Measures to include.
        #[arg(short = 'e', long, num_args = 1..)]
        measures: Vec<String>,

        /// Register data files: name=path pairs (e.g., orders_data=data/orders.csv).
        #[arg(short, long, num_args = 1..)]
        register: Vec<String>,
    },

    /// Start the REST API server.
    #[cfg(feature = "rest")]
    Serve {
        /// Path to YAML manifest file.
        #[arg(short, long)]
        manifest: PathBuf,

        /// Port to bind to.
        #[arg(short, long, default_value = "8080")]
        port: u16,

        /// Host to bind to.
        #[arg(long, default_value = "0.0.0.0")]
        host: String,
    },
}

/// Run the CLI.
pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Compile { input, output } => {
            let yaml = tokio::fs::read_to_string(&input).await?;
            let compiler = ManifestCompiler::new();
            let manifest = compiler
                .compile(CompileSource::Yaml(yaml))
                .await
                .map_err(|e| format!("compilation failed: {}", e))?;
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
            manifest,
            kind,
            dimensions,
            measures,
            json,
        } => {
            let yaml = tokio::fs::read_to_string(&manifest).await?;
            let engine = SemstraitEngine::with_manifest_yaml(&yaml).await?;
            let raw = RawQueryRequest {
                kind,
                dimensions,
                measures,
                ..Default::default()
            };
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
            manifest,
            kind,
            dimensions,
            measures,
        } => {
            let yaml = tokio::fs::read_to_string(&manifest).await?;
            let engine = SemstraitEngine::with_manifest_yaml(&yaml).await?;
            let raw = RawQueryRequest {
                kind,
                dimensions,
                measures,
                ..Default::default()
            };
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
            manifest,
            kind,
            dimensions,
            measures,
            register,
        } => {
            let yaml = tokio::fs::read_to_string(&manifest).await?;
            let compiler = ManifestCompiler::new();
            let compiled = compiler
                .compile(CompileSource::Yaml(yaml))
                .await
                .map_err(|e| format!("compilation failed: {}", e))?;

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
            let raw = RawQueryRequest {
                kind,
                dimensions,
                measures,
                ..Default::default()
            };
            let result = engine.query(&raw).await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }

        #[cfg(feature = "rest")]
        Commands::Serve {
            manifest,
            port,
            host,
        } => {
            let yaml = tokio::fs::read_to_string(&manifest).await?;
            let engine = SemstraitEngine::with_manifest_yaml(&yaml).await?;
            let shared = Arc::new(engine);
            let app = crate::rest::router(shared);
            let addr = format!("{}:{}", host, port);
            eprintln!("Listening on http://{}", addr);
            let listener = tokio::net::TcpListener::bind(&addr).await?;
            axum::serve(listener, app).await?;
            Ok(())
        }
    }
}
