//! CLI transport using clap.
//!
//! Commands:
//! - compile: Compile YAML model files to CompiledManifest
//! - explain: Show query plan and SQL for a query request
//! - validate: Validate a query request against a manifest

use clap::{Parser, Subcommand};
use std::path::PathBuf;

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
        /// Path to YAML model file or directory.
        #[arg(short, long)]
        input: PathBuf,

        /// Output path for the compiled manifest JSON.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Show the query plan and SQL for a query.
    Explain {
        /// Path to compiled manifest JSON.
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
        /// Path to compiled manifest JSON.
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
}

/// Run the CLI.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Compile { input, output } => {
            println!("Compiling: {}", input.display());
            if let Some(out) = output {
                println!("Output: {}", out.display());
            }
            // v1: stub — full implementation connects to ManifestCompiler
            println!("Compilation not yet implemented (v1 stub)");
            Ok(())
        }
        Commands::Explain {
            manifest,
            kind,
            dimensions,
            measures,
            json: _,
        } => {
            println!(
                "Explain: manifest={}, kind={}, dims={:?}, measures={:?}",
                manifest.display(),
                kind,
                dimensions,
                measures
            );
            // v1: stub
            println!("Explain not yet implemented (v1 stub)");
            Ok(())
        }
        Commands::Validate {
            manifest,
            kind,
            dimensions,
            measures,
        } => {
            println!(
                "Validate: manifest={}, kind={}, dims={:?}, measures={:?}",
                manifest.display(),
                kind,
                dimensions,
                measures
            );
            // v1: stub
            println!("Validation not yet implemented (v1 stub)");
            Ok(())
        }
    }
}
