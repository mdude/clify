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

    /// Output the JSON Schema for .clify.yaml (for IDE autocomplete)
    Schema {
        /// Write to file instead of stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        None => {
            // No subcommand — launch TUI
            if atty::is(atty::Stream::Stdout) {
                println!("🚀 Launching Clify TUI... (coming soon)");
                // TODO: launch ratatui TUI in Phase 4
            } else {
                use clap::CommandFactory;
                Cli::command().print_help()?;
            }
        }
        Some(Commands::Init { output }) => {
            println!("📝 Initializing new .clify.yaml in {:?}...", output);
            // TODO: implement in Phase 4 (TUI wizard)
        }
        Some(Commands::Scan { from, source, output }) => {
            println!("🔍 Scanning {} spec from {}...", from, source);
            // TODO: implement in Phase 3
        }
        Some(Commands::Validate { spec }) => {
            validate_spec(&spec)?;
        }
        Some(Commands::Generate { spec, output }) => {
            println!("⚙️  Generating CLI from {:?} into {:?}...", spec, output);
            // TODO: implement in Phase 2
        }
        Some(Commands::Build { release, target: _ }) => {
            let mode = if release { "release" } else { "debug" };
            println!("🔨 Building CLI ({})...", mode);
            // TODO: implement in Phase 5
        }
        Some(Commands::Schema { output }) => {
            let schema = clify_core::schema::generate_json_schema();
            match output {
                Some(path) => {
                    std::fs::write(&path, &schema)?;
                    println!("📄 JSON Schema written to {:?}", path);
                }
                None => {
                    println!("{}", schema);
                }
            }
        }
    }

    Ok(())
}

fn validate_spec(spec_path: &PathBuf) -> anyhow::Result<()> {
    // Read file
    let content = std::fs::read_to_string(spec_path)
        .map_err(|e| anyhow::anyhow!("Failed to read {:?}: {}", spec_path, e))?;

    // Parse YAML
    print!("📄 Parsing {:?}... ", spec_path);
    let spec: clify_core::ClifySpec = match serde_yaml::from_str(&content) {
        Ok(s) => {
            println!("✓");
            s
        }
        Err(e) => {
            println!("✗");
            eprintln!("\n  Parse error: {}", e);
            std::process::exit(2);
        }
    };

    // Validate
    print!("🔍 Validating... ");
    match clify_core::validator::validate(&spec) {
        Ok(()) => {
            println!("✓");
            println!("\n✅ Spec is valid!");
            println!("   Name:     {}", spec.meta.name);
            println!("   Version:  {}", spec.meta.version);
            println!("   Groups:   {}", spec.groups.len());
            println!("   Commands: {}", spec.commands.len());
            let total_params: usize = spec.commands.iter().map(|c| c.params.len()).sum();
            println!("   Params:   {}", total_params);
        }
        Err(errors) => {
            println!("✗");
            eprintln!("\n  Found {} validation error(s):\n", errors.len());
            for (i, err) in errors.iter().enumerate() {
                eprintln!("  {}. {}", i + 1, err);
            }
            std::process::exit(2);
        }
    }

    Ok(())
}
