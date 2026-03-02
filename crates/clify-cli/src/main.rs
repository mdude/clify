use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod tui;

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
                tui::run_tui()?;
            } else {
                use clap::CommandFactory;
                Cli::command().print_help()?;
            }
        }
        Some(Commands::Init { output }) => {
            let spec_path = output.join("api.clify.yaml");
            if spec_path.exists() {
                eprintln!("⚠ {:?} already exists. Remove it first.", spec_path);
                std::process::exit(1);
            }
            let template = r#"# Clify Spec — edit this file to define your CLI
# Docs: https://github.com/mdude/clify/blob/main/docs/CLIFY-SPEC.md

meta:
  name: my-api
  version: "0.1.0"
  description: "CLI for My API"

transport:
  type: rest
  base_url: "https://api.example.com/v1"
  timeout: 30
  retries: 0

auth:
  type: none
  # type: api-key
  # location: header
  # name: "Authorization"
  # env: MY_API_KEY

output:
  default_format: json
  pretty: true

groups: []

commands:
  - name: health
    description: "Health check endpoint"
    request:
      method: GET
      path: "/health"
    response:
      success_status: [200]
"#;
            std::fs::create_dir_all(&output)?;
            std::fs::write(&spec_path, template)?;
            println!("📝 Created {:?}", spec_path);
            println!("   Edit it, then run: clify generate {:?}", spec_path);
        }
        Some(Commands::Scan { from, source, output }) => {
            let content = if source.starts_with("http://") || source.starts_with("https://") {
                println!("🌐 Fetching spec from {}...", source);
                let rt = tokio::runtime::Runtime::new()?;
                let (content, detected_format) = rt.block_on(clify_core::scanner::Scanner::from_url(&source))
                    .map_err(|e| anyhow::anyhow!("{}", e))?;
                println!("   Detected format: {}", detected_format);
                content
            } else {
                std::fs::read_to_string(&source)
                    .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", source, e))?
            };

            println!("🔍 Scanning {} spec...", from);
            let spec = match from.as_str() {
                "openapi" => clify_core::scanner::Scanner::from_openapi(&content),
                "swagger" => clify_core::scanner::Scanner::from_swagger(&content),
                _ => Err(clify_core::scanner::ScanError::UnsupportedFormat(from.clone())),
            }.map_err(|e| anyhow::anyhow!("{}", e))?;

            let yaml = clify_core::scanner::Scanner::to_yaml(&spec)
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            std::fs::write(&output, &yaml)?;

            println!("✅ Generated {:?}", output);
            println!("   Name:     {}", spec.meta.name);
            println!("   Commands: {}", spec.commands.len());
            println!("   Groups:   {}", spec.groups.len());
            println!("\n   Review and curate the spec, then run: clify generate {:?}", output);
        }
        Some(Commands::Validate { spec }) => {
            validate_spec(&spec)?;
        }
        Some(Commands::Generate { spec, output }) => {
            // Parse and validate first
            let content = std::fs::read_to_string(&spec)
                .map_err(|e| anyhow::anyhow!("Failed to read {:?}: {}", spec, e))?;
            let parsed: clify_core::ClifySpec = serde_yaml::from_str(&content)
                .map_err(|e| anyhow::anyhow!("Parse error: {}", e))?;

            if let Err(errors) = clify_core::validator::validate(&parsed) {
                eprintln!("Spec validation failed:");
                for err in &errors {
                    eprintln!("  ✗ {}", err);
                }
                std::process::exit(2);
            }

            let generator = clify_core::generator::Generator::new(parsed);
            generator.generate(&output)
                .map_err(|e| anyhow::anyhow!("Generation failed: {}", e))?;

            println!("✅ Generated CLI project at {:?}", output.join(
                &serde_yaml::from_str::<clify_core::ClifySpec>(&content).unwrap().meta.name
            ));
            println!("   Next: cd {} && cargo build --release", 
                serde_yaml::from_str::<clify_core::ClifySpec>(&content).unwrap().meta.name);
        }
        Some(Commands::Build { release, target }) => {
            let mut cmd = std::process::Command::new("cargo");
            cmd.arg("build");
            if release {
                cmd.arg("--release");
            }
            if let Some(ref t) = target {
                cmd.arg("--target").arg(t);
            }
            println!("🔨 Building...");
            let status = cmd.status()?;
            if status.success() {
                println!("✅ Build succeeded!");
            } else {
                std::process::exit(status.code().unwrap_or(1));
            }
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
