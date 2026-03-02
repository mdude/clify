use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "clify",
    version,
    about = "Clify makes your software cliable.",
    long_about = "Generate fully-featured CLI tools from API specifications.\n\nRun without arguments to launch the interactive TUI."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new .clify.yaml spec file
    Init {
        /// Output directory
        #[arg(short, long, default_value = ".")]
        output: PathBuf,
    },

    /// Scan an API spec and generate a .clify.yaml
    Scan {
        /// Source type
        #[arg(long, value_parser = ["openapi", "swagger"])]
        from: String,

        /// Path or URL to the API spec
        source: String,

        /// Output file
        #[arg(short, long, default_value = "api.clify.yaml")]
        output: PathBuf,
    },

    /// Validate a .clify.yaml spec file
    Validate {
        /// Path to the spec file
        spec: PathBuf,
    },

    /// Generate a Rust CLI project from a .clify.yaml spec
    Generate {
        /// Path to the spec file
        spec: PathBuf,

        /// Output directory for the generated project
        #[arg(short, long, default_value = ".")]
        output: PathBuf,
    },

    /// Build the generated CLI project
    Build {
        /// Build in release mode
        #[arg(long)]
        release: bool,

        /// Target triple for cross-compilation
        #[arg(long)]
        target: Option<String>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        None => {
            // No subcommand — launch TUI
            if atty::is(atty::Stream::Stdout) {
                println!("🚀 Launching Clify TUI... (coming soon)");
                // TODO: launch ratatui TUI
            } else {
                // Not a TTY — print help
                use clap::CommandFactory;
                Cli::command().print_help()?;
            }
        }
        Some(Commands::Init { output }) => {
            println!("📝 Initializing new .clify.yaml in {:?}...", output);
            // TODO: implement
        }
        Some(Commands::Scan { from, source, output }) => {
            println!("🔍 Scanning {} spec from {}...", from, source);
            // TODO: implement
        }
        Some(Commands::Validate { spec }) => {
            println!("✅ Validating {:?}...", spec);
            let content = std::fs::read_to_string(&spec)?;
            let parsed: clify_core::ClifySpec = serde_yaml::from_str(&content)?;
            match clify_core::validator::validate(&parsed) {
                Ok(()) => println!("  Spec is valid!"),
                Err(errors) => {
                    println!("  Found {} error(s):", errors.len());
                    for err in errors {
                        println!("    ✗ {}", err);
                    }
                    std::process::exit(2);
                }
            }
        }
        Some(Commands::Generate { spec, output }) => {
            println!("⚙️  Generating CLI from {:?} into {:?}...", spec, output);
            // TODO: implement
        }
        Some(Commands::Build { release, target }) => {
            let mode = if release { "release" } else { "debug" };
            println!("🔨 Building CLI ({})...", mode);
            // TODO: implement
        }
    }

    Ok(())
}
