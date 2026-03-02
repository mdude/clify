//! Code generator — transforms a validated ClifySpec into a Rust project.

use crate::spec::*;
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GeneratorError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Template error: {0}")]
    Template(String),
}

pub struct Generator {
    spec: ClifySpec,
}

impl Generator {
    pub fn new(spec: ClifySpec) -> Self {
        Self { spec }
    }

    /// Generate the full Rust project into the output directory.
    pub fn generate(&self, output_dir: &Path) -> Result<(), GeneratorError> {
        let project_dir = output_dir.join(&self.spec.meta.name);
        std::fs::create_dir_all(project_dir.join("src"))?;

        self.write_cargo_toml(&project_dir)?;
        self.write_main_rs(&project_dir)?;
        self.write_commands_rs(&project_dir)?;

        Ok(())
    }

    fn write_cargo_toml(&self, dir: &Path) -> Result<(), GeneratorError> {
        // Sanitize description for Cargo.toml (escape quotes, truncate)
        let desc = self.spec.meta.description
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', " ");
        let desc = if desc.len() > 200 { format!("{}...", &desc[..197]) } else { desc };

        let mut content = format!(
            r#"[package]
name = "{name}"
version = "{version}"
edition = "2021"
description = "{description}"
"#,
            name = self.spec.meta.name,
            version = self.spec.meta.version,
            description = desc,
        );

        if let Some(ref author) = self.spec.meta.author {
            content.push_str(&format!("authors = [\"{}\"]\n", author));
        }
        if let Some(ref license) = self.spec.meta.license {
            content.push_str(&format!("license = \"{}\"\n", license));
        }

        content.push_str(&format!(
            r#"
[[bin]]
name = "{}"
path = "src/main.rs"

[dependencies]
clap = {{ version = "4", features = ["derive"] }}
tokio = {{ version = "1", features = ["full"] }}
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
reqwest = {{ version = "0.12", features = ["json", "rustls-tls"], default-features = false }}
anyhow = "1"
dirs = "6"
toml = "0.8"
comfy-table = "7"
csv = "1"
"#,
            self.spec.meta.name
        ));

        std::fs::write(dir.join("Cargo.toml"), content)?;
        Ok(())
    }

    fn write_main_rs(&self, dir: &Path) -> Result<(), GeneratorError> {
        let mut code = String::new();

        // Imports
        code.push_str("use clap::{Parser, Subcommand};\n");
        code.push_str("use std::collections::HashMap;\n\n");
        code.push_str("mod commands;\n\n");

        // Top-level CLI struct
        code.push_str(&format!(
            r#"#[derive(Parser)]
#[command(
    name = "{}",
    version = "{}",
    about = "{}"
)]
struct Cli {{"#,
            self.spec.meta.name,
            self.spec.meta.version,
            sanitize_string(&self.spec.meta.description),
        ));

        let default_fmt = match self.spec.output.default_format {
            OutputFormat::Json => "json",
            OutputFormat::Table => "table",
            OutputFormat::Csv => "csv",
        };
        code.push_str(&format!(r#"
    #[command(subcommand)]
    command: Commands,

    /// Output format
    #[arg(short, long, global = true, default_value = "{default_fmt}")]
    output: String,

    /// Disable pretty-printing
    #[arg(long, global = true)]
    no_pretty: bool,

    /// Show HTTP request without executing
    #[arg(long, global = true)]
    dry_run: bool,

    /// Show request/response details
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Override base URL
    #[arg(long, global = true)]
    base_url: Option<String>,

    /// Override auth token
    #[arg(long, global = true)]
    token: Option<String>,

    /// Request timeout in seconds
    #[arg(long, global = true)]
    timeout: Option<u64>,
}}

"#));

        // Commands enum
        self.generate_commands_enum(&mut code);

        // Main function
        code.push_str(r#"
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let ctx = commands::Context::new(&cli);

    match cli.command {
"#);

        // Auth subcommands
        code.push_str("        Commands::Auth { action } => commands::handle_auth(&ctx, action).await?,\n");
        code.push_str("        Commands::Config { action } => commands::handle_config(&ctx, action)?,\n");

        // Route to command handlers
        if self.spec.groups.is_empty() {
            for cmd in &self.spec.commands {
                let variant = to_pascal_case(&cmd.name);
                let fields = self.command_field_names(cmd);
                code.push_str(&format!(
                    "        Commands::{variant} {{ {fields} }} => commands::cmd_{fn_name}(&ctx, {fields}).await?,\n",
                    variant = variant,
                    fields = fields,
                    fn_name = cmd.name.replace('-', "_"),
                ));
            }
        } else {
            // Grouped commands
            let mut grouped: HashMap<String, Vec<&Command>> = HashMap::new();
            let mut ungrouped: Vec<&Command> = Vec::new();
            for cmd in &self.spec.commands {
                if let Some(ref group) = cmd.group {
                    grouped.entry(group.clone()).or_default().push(cmd);
                } else {
                    ungrouped.push(cmd);
                }
            }

            for group in &self.spec.groups {
                let group_variant = to_pascal_case(&group.name);
                code.push_str(&format!(
                    "        Commands::{} {{ command }} => {{\n            match command {{\n",
                    group_variant
                ));
                if let Some(cmds) = grouped.get(&group.name) {
                    for cmd in cmds {
                        let variant = to_pascal_case(&cmd.name);
                        let fields = self.command_field_names(cmd);
                        code.push_str(&format!(
                            "                {}Commands::{} {{ {} }} => commands::cmd_{}_{}(&ctx, {}).await?,\n",
                            group_variant, variant, fields,
                            group.name.replace('-', "_"), cmd.name.replace('-', "_"), fields,
                        ));
                    }
                }
                code.push_str("            }\n        }\n");
            }

            for cmd in &ungrouped {
                let variant = to_pascal_case(&cmd.name);
                let fields = self.command_field_names(cmd);
                code.push_str(&format!(
                    "        Commands::{variant} {{ {fields} }} => commands::cmd_{fn_name}(&ctx, {fields}).await?,\n",
                    variant = variant,
                    fields = fields,
                    fn_name = cmd.name.replace('-', "_"),
                ));
            }
        }

        code.push_str("    }\n\n    Ok(())\n}\n");

        std::fs::write(dir.join("src/main.rs"), code)?;
        Ok(())
    }

    fn generate_commands_enum(&self, code: &mut String) {
        code.push_str("#[derive(Subcommand)]\nenum Commands {\n");

        // Auth commands
        code.push_str("    /// Authentication management\n");
        code.push_str("    Auth {\n        #[command(subcommand)]\n        action: AuthAction,\n    },\n");

        // Config commands
        code.push_str("    /// Configuration management\n");
        code.push_str("    Config {\n        #[command(subcommand)]\n        action: ConfigAction,\n    },\n");

        if self.spec.groups.is_empty() {
            // Flat commands
            for cmd in &self.spec.commands {
                self.generate_command_variant(code, cmd, "");
            }
        } else {
            // Group subcommands
            let mut grouped: HashMap<String, Vec<&Command>> = HashMap::new();
            let mut ungrouped: Vec<&Command> = Vec::new();
            for cmd in &self.spec.commands {
                if let Some(ref group) = cmd.group {
                    grouped.entry(group.clone()).or_default().push(cmd);
                } else {
                    ungrouped.push(cmd);
                }
            }

            for group in &self.spec.groups {
                let variant = to_pascal_case(&group.name);
                code.push_str(&format!("    /// {}\n", group.description));
                code.push_str(&format!(
                    "    {} {{\n        #[command(subcommand)]\n        command: {}Commands,\n    }},\n",
                    variant, variant
                ));
            }

            for cmd in &ungrouped {
                self.generate_command_variant(code, cmd, "");
            }
        }

        code.push_str("}\n\n");

        // Auth/Config action enums
        code.push_str(r#"#[derive(Subcommand)]
enum AuthAction {
    /// Log in and store credentials
    Login,
    /// Show current auth status
    Status,
    /// Clear stored credentials
    Logout,
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Set a config value
    Set {
        key: String,
        value: String,
    },
    /// Get a config value
    Get {
        key: String,
    },
    /// List all config values
    List,
    /// Reset config to defaults
    Reset,
}

"#);

        // Group sub-enums
        if !self.spec.groups.is_empty() {
            let mut grouped: HashMap<String, Vec<&Command>> = HashMap::new();
            for cmd in &self.spec.commands {
                if let Some(ref group) = cmd.group {
                    grouped.entry(group.clone()).or_default().push(cmd);
                }
            }

            for group in &self.spec.groups {
                let group_variant = to_pascal_case(&group.name);
                code.push_str(&format!("#[derive(Subcommand)]\nenum {}Commands {{\n", group_variant));
                if let Some(cmds) = grouped.get(&group.name) {
                    for cmd in cmds {
                        self.generate_command_variant(code, cmd, "");
                    }
                }
                code.push_str("}\n\n");
            }
        }
    }

    fn generate_command_variant(&self, code: &mut String, cmd: &Command, _prefix: &str) {
        code.push_str(&format!("    /// {}\n", sanitize_string(&cmd.description)));
        if !cmd.aliases.is_empty() {
            let aliases: Vec<String> = cmd.aliases.iter().map(|a| format!("\"{}\"", a)).collect();
            code.push_str(&format!("    #[command(alias{} = {})]\n",
                if cmd.aliases.len() > 1 { "es" } else { "" },
                if cmd.aliases.len() > 1 {
                    format!("[{}]", aliases.join(", "))
                } else {
                    aliases[0].clone()
                }
            ));
        }
        let variant = to_pascal_case(&cmd.name);
        code.push_str(&format!("    {} {{\n", variant));

        for param in &cmd.params {
            if param.hidden {
                continue; // Hidden params get hardcoded defaults
            }
            self.generate_param_field(code, param);
        }

        code.push_str("    },\n");
    }

    fn generate_param_field(&self, code: &mut String, param: &Param) {
        code.push_str(&format!("        /// {}\n", sanitize_string(&param.description)));

        let mut attrs = Vec::new();
        // clap auto-derives --long-name from field_name (replacing _ with -)
        // Only specify long explicitly if param name differs from field name's kebab form
        let field = safe_ident(&param.name);
        let auto_long = field.replace('_', "-").trim_start_matches("r#").to_string();
        if auto_long != param.name {
            attrs.push(format!("long = {:?}", param.name));
        }
        if let Some(ref short) = param.short {
            attrs.push(format!("short = '{}'", short));
        }

        let rust_type = self.param_rust_type(param);

        if let Some(ref default) = param.default {
            // clap default_value always takes a string
            let default_str = match default {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                _ => default.to_string(),
            };
            attrs.push(format!("default_value = \"{}\"", default_str));
        }

        if let Some(ref env) = param.env {
            attrs.push(format!("env = \"{}\"", env));
        }

        if matches!(param.param_type, ParamType::Enum) && !param.values.is_empty() {
            let values: Vec<String> = param.values.iter().map(|v| format!("\"{}\"", v)).collect();
            // Don't use value_parser array syntax — causes parse issues in newer Rust
            // The enum values are documented in help text via description
        }

        code.push_str(&format!("        #[arg({})]\n", attrs.join(", ")));

        let field_name = safe_ident(&param.name);
        if param.required && param.default.is_none() {
            code.push_str(&format!("        {}: {},\n", field_name, rust_type));
        } else {
            code.push_str(&format!("        {}: Option<{}>,\n", field_name, rust_type));
        }
    }

    fn param_rust_type(&self, param: &Param) -> String {
        match param.param_type {
            ParamType::String | ParamType::Enum => "String".to_string(),
            ParamType::Integer => "i64".to_string(),
            ParamType::Float => "f64".to_string(),
            ParamType::Boolean => "bool".to_string(),
            ParamType::Array => "String".to_string(), // Split by separator at runtime
            ParamType::File => "String".to_string(),  // File path
            ParamType::Object => "String".to_string(), // JSON string
        }
    }

    fn command_field_names(&self, cmd: &Command) -> String {
        cmd.params.iter()
            .filter(|p| !p.hidden)
            .map(|p| safe_ident(&p.name))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn write_commands_rs(&self, dir: &Path) -> Result<(), GeneratorError> {
        let mut code = String::new();

        code.push_str("use std::collections::HashMap;\n\n");

        // Context struct
        code.push_str(&format!(r#"pub struct Context {{
    pub base_url: String,
    pub timeout: u64,
    pub output_format: String,
    pub pretty: bool,
    pub dry_run: bool,
    pub verbose: bool,
    pub token: Option<String>,
    pub config_path: std::path::PathBuf,
    pub auth_path: std::path::PathBuf,
}}

impl Context {{
    pub fn new(cli: &super::Cli) -> Self {{
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("{}");
        let config_path = config_dir.join("config.toml");

        // Load user config
        let user_config: HashMap<String, String> = if config_path.exists() {{
            std::fs::read_to_string(&config_path)
                .ok()
                .and_then(|c| toml::from_str(&c).ok())
                .unwrap_or_default()
        }} else {{
            HashMap::new()
        }};

        let base_url = cli.base_url.clone()
            .or_else(|| user_config.get("base_url").cloned())
            .unwrap_or_else(|| "{}".to_string());

        let timeout = cli.timeout
            .or_else(|| user_config.get("timeout").and_then(|t| t.parse().ok()))
            .unwrap_or({});

        let output_format = user_config.get("output_format")
            .cloned()
            .unwrap_or_else(|| cli.output.clone());

        let pretty = user_config.get("pretty")
            .and_then(|p| p.parse().ok())
            .unwrap_or(!cli.no_pretty);

        Self {{
            base_url,
            timeout,
            output_format,
            pretty,
            dry_run: cli.dry_run,
            verbose: cli.verbose,
            token: cli.token.clone(),
            config_path,
            auth_path: config_dir.join("auth.json"),
        }}
    }}
}}

"#,
            self.spec.meta.name,
            self.spec.transport.base_url,
            self.spec.transport.timeout,
        ));

        // Auth handler
        self.generate_auth_handler(&mut code);

        // Config handler
        self.generate_config_handler(&mut code);

        // HTTP helper
        self.generate_http_helper(&mut code);

        // Command handlers
        if self.spec.groups.is_empty() {
            for cmd in &self.spec.commands {
                self.generate_command_handler(&mut code, cmd, None)?;
            }
        } else {
            let mut grouped: HashMap<String, Vec<&Command>> = HashMap::new();
            let mut ungrouped: Vec<&Command> = Vec::new();
            for cmd in &self.spec.commands {
                if let Some(ref group) = cmd.group {
                    grouped.entry(group.clone()).or_default().push(cmd);
                } else {
                    ungrouped.push(cmd);
                }
            }

            for group in &self.spec.groups {
                if let Some(cmds) = grouped.get(&group.name) {
                    for cmd in cmds {
                        self.generate_command_handler(&mut code, cmd, Some(&group.name))?;
                    }
                }
            }

            for cmd in &ungrouped {
                self.generate_command_handler(&mut code, cmd, None)?;
            }
        }

        std::fs::write(dir.join("src/commands.rs"), code)?;
        Ok(())
    }

    fn generate_auth_handler(&self, code: &mut String) {
        code.push_str(r#"pub async fn handle_auth(ctx: &Context, action: super::AuthAction) -> anyhow::Result<()> {
    match action {
        super::AuthAction::Login => {
"#);

        match &self.spec.auth {
            Auth::Oauth2 { token_url, env_client_id, env_client_secret, custom, .. } => {
                let token_field = custom.as_ref().map(|c| c.token_field.as_str()).unwrap_or("access_token");
                let expiry_field = custom.as_ref().map(|c| c.expiry_field.as_str()).unwrap_or("expires_in");
                let use_form = custom.as_ref().map(|c| matches!(c.content_type, ContentType::Form)).unwrap_or(false);

                code.push_str(&format!(r#"            let client_id = std::env::var("{}")
                .map_err(|_| anyhow::anyhow!("Set {} environment variable"))?;
            let client_secret = std::env::var("{}")
                .map_err(|_| anyhow::anyhow!("Set {} environment variable"))?;

            let client = reqwest::Client::new();
            let mut params = HashMap::new();
            params.insert("grant_type", "client_credentials".to_string());
            params.insert("client_id", client_id);
            params.insert("client_secret", client_secret);
"#,
                    env_client_id, env_client_id, env_client_secret, env_client_secret,
                ));

                // Extra params
                if let Some(c) = custom {
                    for (k, v) in &c.extra_params {
                        code.push_str(&format!(
                            "            params.insert(\"{}\", \"{}\".to_string());\n", k, v
                        ));
                    }
                }

                code.push_str(&format!(r#"
            let resp = client.post("{}")
                .{}(&params)
                .send().await?;
            let body: serde_json::Value = resp.json().await?;

            if let Some(err) = body.get("error") {{
                anyhow::bail!("Auth failed: {{}}", err);
            }}

            let token = body.get("{}")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("No token in response"))?;

            // Save token
            if let Some(parent) = ctx.auth_path.parent() {{
                std::fs::create_dir_all(parent)?;
            }}
            let creds = serde_json::json!({{
                "token": token,
                "expires_at": body.get("{}").and_then(|v| v.as_u64()).map(|e| {{
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH).unwrap()
                        .as_secs() + e
                }})
            }});
            std::fs::write(&ctx.auth_path, serde_json::to_string_pretty(&creds)?)?;
            println!("✓ Authenticated successfully.");
"#,
                    token_url,
                    if use_form { "form" } else { "json" },
                    token_field,
                    expiry_field,
                ));
            }
            Auth::ApiKey { env, .. } | Auth::Token { env } => {
                code.push_str(&format!(r#"            println!("Enter your token (or set {} env var):");
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            let token = input.trim().to_string();
            if let Some(parent) = ctx.auth_path.parent() {{
                std::fs::create_dir_all(parent)?;
            }}
            let creds = serde_json::json!({{"token": token}});
            std::fs::write(&ctx.auth_path, serde_json::to_string_pretty(&creds)?)?;
            println!("✓ Credentials saved.");
"#, env));
            }
            Auth::None | Auth::Basic { .. } => {
                code.push_str("            println!(\"No interactive login required.\");\n");
            }
        }

        code.push_str(r#"        }
        super::AuthAction::Status => {
            if ctx.auth_path.exists() {
                let data: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&ctx.auth_path)?)?;
                if let Some(token) = data.get("token").and_then(|t| t.as_str()) {
                    let preview = if token.len() > 12 {
                        format!("{}...{}", &token[..6], &token[token.len()-4..])
                    } else {
                        "****".to_string()
                    };
                    println!("✓ Authenticated (token: {})", preview);
                } else {
                    println!("✗ Not authenticated");
                }
            } else {
                println!("✗ Not authenticated");
            }
        }
        super::AuthAction::Logout => {
            if ctx.auth_path.exists() {
                std::fs::remove_file(&ctx.auth_path)?;
                println!("✓ Credentials cleared.");
            } else {
                println!("No credentials stored.");
            }
        }
    }
    Ok(())
}

"#);
    }

    fn generate_config_handler(&self, code: &mut String) {
        code.push_str(r#"pub fn handle_config(ctx: &Context, action: super::ConfigAction) -> anyhow::Result<()> {
    let mut config: HashMap<String, String> = if ctx.config_path.exists() {
        let content = std::fs::read_to_string(&ctx.config_path)?;
        toml::from_str(&content).unwrap_or_default()
    } else {
        HashMap::new()
    };

    match action {
        super::ConfigAction::Set { key, value } => {
            let valid_keys = ["base_url", "output_format", "timeout", "pretty"];
            if !valid_keys.contains(&key.as_str()) {
                anyhow::bail!("Unknown config key '{}'. Valid keys: {}", key, valid_keys.join(", "));
            }
            config.insert(key.clone(), value.clone());
            if let Some(parent) = ctx.config_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&ctx.config_path, toml::to_string_pretty(&config)?)?;
            println!("✓ {} = {}", key, value);
        }
        super::ConfigAction::Get { key } => {
            match config.get(&key) {
                Some(val) => println!("{} = {}", key, val),
                None => println!("{} = (not set)", key),
            }
        }
        super::ConfigAction::List => {
            if config.is_empty() {
                println!("(no config set)");
            } else {
                for (k, v) in &config {
                    println!("{} = {}", k, v);
                }
            }
        }
        super::ConfigAction::Reset => {
            if ctx.config_path.exists() {
                std::fs::remove_file(&ctx.config_path)?;
            }
            println!("✓ Config reset to defaults.");
        }
    }
    Ok(())
}

"#);
    }

    fn generate_http_helper(&self, code: &mut String) {
        // Generate a helper that resolves auth token
        code.push_str(r#"fn resolve_token(ctx: &Context) -> Option<String> {
    // 1. Explicit --token flag
    if let Some(ref token) = ctx.token {
        return Some(token.clone());
    }
"#);

        // 2. Env var
        match &self.spec.auth {
            Auth::ApiKey { env, .. } | Auth::Token { env } => {
                code.push_str(&format!(
                    "    // 2. Env var\n    if let Ok(token) = std::env::var(\"{}\") {{\n        return Some(token);\n    }}\n",
                    env
                ));
            }
            _ => {}
        }

        // 3. Stored credentials
        code.push_str(r#"    // 3. Stored credentials
    if ctx.auth_path.exists() {
        if let Ok(data) = std::fs::read_to_string(&ctx.auth_path) {
            if let Ok(creds) = serde_json::from_str::<serde_json::Value>(&data) {
                return creds.get("token").and_then(|t| t.as_str()).map(|s| s.to_string());
            }
        }
    }
    None
}

async fn do_request(
    ctx: &Context,
    method: &str,
    path: &str,
    query: &HashMap<String, String>,
    body: &HashMap<String, serde_json::Value>,
    content_type: &str,
) -> anyhow::Result<serde_json::Value> {
    let url = format!("{}{}", ctx.base_url.trim_end_matches('/'), path);

    if ctx.dry_run {
        println!("{} {}", method, url);
        if !query.is_empty() {
            println!("Query: {:?}", query);
        }
        if !body.is_empty() {
            println!("Body: {}", serde_json::to_string_pretty(body)?);
        }
        return Ok(serde_json::Value::Null);
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(ctx.timeout))
        .build()?;

    let method_parsed = method.parse::<reqwest::Method>()?;
    let mut builder = client.request(method_parsed, &url);
"#);

        // Add default headers
        for (k, v) in &self.spec.transport.headers {
            code.push_str(&format!(
                "    builder = builder.header(\"{}\", \"{}\");\n", k, v
            ));
        }

        code.push_str(r#"
    // Query params
    if !query.is_empty() {
        builder = builder.query(query);
    }

    // Body
    if !body.is_empty() {
        match content_type {
            "form" => {
                let form: HashMap<String, String> = body.iter()
                    .map(|(k, v)| (k.clone(), match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    }))
                    .collect();
                builder = builder.form(&form);
            }
            _ => {
                builder = builder.json(body);
            }
        }
    }

    // Auth
    if let Some(token) = resolve_token(ctx) {
"#);

        // Apply auth based on type
        match &self.spec.auth {
            Auth::ApiKey { location, name, .. } => {
                match location {
                    ApiKeyLocation::Header => {
                        code.push_str(&format!(
                            "        builder = builder.header(\"{}\", &token);\n", name
                        ));
                    }
                    ApiKeyLocation::Query => {
                        code.push_str(&format!(
                            "        builder = builder.query(&[(\"{}\", &token)]);\n", name
                        ));
                    }
                }
            }
            Auth::Basic { .. } => {
                code.push_str("        // Basic auth handled separately\n");
            }
            _ => {
                code.push_str("        builder = builder.bearer_auth(&token);\n");
            }
        }

        code.push_str(r#"    }

    if ctx.verbose {
        eprintln!("→ {} {}", method, url);
    }

    let resp = builder.send().await?;
    let status = resp.status();
    let body_text = resp.text().await?;

    if ctx.verbose {
        eprintln!("← {} ({} bytes)", status, body_text.len());
    }

    let json: serde_json::Value = serde_json::from_str(&body_text)
        .unwrap_or_else(|_| serde_json::Value::String(body_text));

    if !status.is_success() {
        anyhow::bail!("HTTP {}: {}", status.as_u16(), serde_json::to_string_pretty(&json)?);
    }

    Ok(json)
}

fn format_output(ctx: &Context, value: &serde_json::Value, success_path: Option<&str>) {
    let data = if let Some(path) = success_path {
        extract_json_path(value, path).unwrap_or(value.clone())
    } else {
        value.clone()
    };

    match ctx.output_format.as_str() {
        "table" => print_table(&data),
        "csv" => print_csv(&data),
        _ => {
            if ctx.pretty {
                println!("{}", serde_json::to_string_pretty(&data).unwrap_or_default());
            } else {
                println!("{}", serde_json::to_string(&data).unwrap_or_default());
            }
        }
    }
}

fn extract_json_path(value: &serde_json::Value, path: &str) -> Option<serde_json::Value> {
    let mut current = value.clone();
    for part in path.split('.') {
        if let Some(bracket_pos) = part.find('[') {
            let key = &part[..bracket_pos];
            let idx_str = &part[bracket_pos + 1..part.len() - 1];
            if !key.is_empty() {
                current = current.get(key)?.clone();
            }
            let idx: usize = idx_str.parse().ok()?;
            current = current.get(idx)?.clone();
        } else {
            current = current.get(part)?.clone();
        }
    }
    Some(current)
}

fn print_table(value: &serde_json::Value) {
    let rows = match value {
        serde_json::Value::Array(arr) => arr.clone(),
        obj @ serde_json::Value::Object(_) => vec![obj.clone()],
        _ => { println!("{}", value); return; }
    };
    if rows.is_empty() { println!("(no results)"); return; }

    let mut headers = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for row in &rows {
        if let serde_json::Value::Object(map) = row {
            for key in map.keys() {
                if seen.insert(key.clone()) { headers.push(key.clone()); }
            }
        }
    }

    let mut table = comfy_table::Table::new();
    table.set_header(&headers);
    table.load_preset(comfy_table::presets::UTF8_FULL_CONDENSED);
    for row in &rows {
        let cells: Vec<String> = headers.iter().map(|h| {
            row.get(h).map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Null => "".to_string(),
                other => other.to_string(),
            }).unwrap_or_default()
        }).collect();
        table.add_row(cells);
    }
    println!("{}", table);
}

fn print_csv(value: &serde_json::Value) {
    let rows = match value {
        serde_json::Value::Array(arr) => arr.clone(),
        obj @ serde_json::Value::Object(_) => vec![obj.clone()],
        _ => { println!("{}", value); return; }
    };
    if rows.is_empty() { return; }

    let mut headers = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for row in &rows {
        if let serde_json::Value::Object(map) = row {
            for key in map.keys() {
                if seen.insert(key.clone()) { headers.push(key.clone()); }
            }
        }
    }

    let mut wtr = csv::Writer::from_writer(std::io::stdout());
    let _ = wtr.write_record(&headers);
    for row in &rows {
        let record: Vec<String> = headers.iter().map(|h| {
            row.get(h).map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Null => "".to_string(),
                other => other.to_string(),
            }).unwrap_or_default()
        }).collect();
        let _ = wtr.write_record(&record);
    }
    let _ = wtr.flush();
}

"#);
    }

    fn generate_command_handler(&self, code: &mut String, cmd: &Command, group: Option<&str>) -> Result<(), GeneratorError> {
        let fn_name = if let Some(g) = group {
            format!("cmd_{}_{}", g.replace('-', "_"), cmd.name.replace('-', "_"))
        } else {
            format!("cmd_{}", cmd.name.replace('-', "_"))
        };

        // Function signature
        let visible_params: Vec<&Param> = cmd.params.iter().filter(|p| !p.hidden).collect();
        let params_sig: Vec<String> = visible_params.iter().map(|p| {
            let field = safe_ident(&p.name);
            let rust_type = match p.param_type {
                ParamType::String | ParamType::Enum | ParamType::Array | ParamType::File | ParamType::Object => "String",
                ParamType::Integer => "i64",
                ParamType::Float => "f64",
                ParamType::Boolean => "bool",
            };
            if p.required && p.default.is_none() {
                format!("{}: {}", field, rust_type)
            } else {
                format!("{}: Option<{}>", field, rust_type)
            }
        }).collect();

        code.push_str(&format!(
            "pub async fn {}(ctx: &Context, {}) -> anyhow::Result<()> {{\n",
            fn_name, params_sig.join(", ")
        ));

        // Build path with interpolation
        let mut path_expr = format!("\"{}\"", cmd.request.path);
        let path_params: Vec<&Param> = cmd.params.iter()
            .filter(|p| matches!(p.source, Some(ParamSource::Path)))
            .collect();
        if !path_params.is_empty() {
            path_expr = format!("format!(\"{}\"", cmd.request.path.clone());
            for pp in &path_params {
                let field = safe_ident(&pp.name);
                let placeholder = format!("{{{}}}", pp.name);
                path_expr = path_expr.replace(&placeholder, "{}");
                // If it's visible and optional, unwrap with default
                if pp.hidden {
                    if let Some(ref default) = pp.default {
                        path_expr.push_str(&format!(", \"{}\"", default));
                    }
                } else if pp.required && pp.default.is_none() {
                    path_expr.push_str(&format!(", {}", field));
                } else {
                    let default_val = pp.default.as_ref()
                        .map(|d| match d {
                            serde_json::Value::String(s) => format!("\"{}\"", s),
                            serde_json::Value::Number(n) => n.to_string(),
                            _ => "\"\"".to_string(),
                        })
                        .unwrap_or_else(|| "\"\"".to_string());
                    path_expr.push_str(&format!(", {}.map(|v| v.to_string()).unwrap_or_else(|| {}.to_string())", field, default_val));
                }
            }
            path_expr.push(')');
        }
        code.push_str(&format!("    let path = {};\n", path_expr));

        // Build query params
        code.push_str("    let mut query = HashMap::new();\n");
        for param in &cmd.params {
            let source = param.source.clone().unwrap_or_else(|| {
                match cmd.request.method {
                    HttpMethod::Get | HttpMethod::Delete => ParamSource::Query,
                    _ => ParamSource::Body,
                }
            });
            if !matches!(source, ParamSource::Query) { continue; }
            if matches!(param.source, Some(ParamSource::Path)) { continue; }

            let field = safe_ident(&param.name);
            if param.hidden {
                if let Some(ref default) = param.default {
                    if let serde_json::Value::String(s) = default {
                        code.push_str(&format!("    query.insert(\"{}\".to_string(), \"{}\".to_string());\n", param.name, s));
                    }
                }
            } else if param.required && param.default.is_none() {
                code.push_str(&format!("    query.insert(\"{}\".to_string(), {}.to_string());\n", param.name, field));
            } else {
                code.push_str(&format!("    if let Some(ref val) = {} {{\n        query.insert(\"{}\".to_string(), val.to_string());\n    }}\n", field, param.name));
                if let Some(ref default) = param.default {
                    if let serde_json::Value::String(s) = default {
                        code.push_str(&format!("    else {{\n        query.insert(\"{}\".to_string(), \"{}\".to_string());\n    }}\n", param.name, s));
                    }
                }
            }
        }

        // Build body params
        code.push_str("    let mut body = HashMap::new();\n");
        for param in &cmd.params {
            let source = param.source.clone().unwrap_or_else(|| {
                match cmd.request.method {
                    HttpMethod::Get | HttpMethod::Delete => ParamSource::Query,
                    _ => ParamSource::Body,
                }
            });
            if !matches!(source, ParamSource::Body) { continue; }

            let field = safe_ident(&param.name);
            if param.hidden {
                if let Some(ref default) = param.default {
                    code.push_str(&format!("    body.insert(\"{}\".to_string(), serde_json::json!({}));\n",
                        param.name,
                        match default {
                            serde_json::Value::String(s) => format!("\"{}\"", s),
                            other => other.to_string(),
                        }
                    ));
                }
            } else if matches!(param.param_type, ParamType::File) {
                // Read file contents
                if param.required && param.default.is_none() {
                    code.push_str(&format!(r#"    let file_content = std::fs::read_to_string(&{})?;
    body.insert("{}".to_string(), serde_json::Value::String(file_content));
"#, field, param.name));
                } else {
                    code.push_str(&format!(r#"    if let Some(ref path) = {} {{
        let file_content = std::fs::read_to_string(path)?;
        body.insert("{}".to_string(), serde_json::Value::String(file_content));
    }}
"#, field, param.name));
                }
            } else if param.required && param.default.is_none() {
                let json_val = match param.param_type {
                    ParamType::Integer | ParamType::Float => format!("serde_json::json!({})", field),
                    ParamType::Boolean => format!("serde_json::json!({})", field),
                    _ => format!("serde_json::json!({})", field),
                };
                code.push_str(&format!("    body.insert(\"{}\".to_string(), {});\n", param.name, json_val));
            } else {
                code.push_str(&format!("    if let Some(ref val) = {} {{\n        body.insert(\"{}\".to_string(), serde_json::json!(val));\n    }}\n", field, param.name));
                if let Some(ref default) = param.default {
                    code.push_str(&format!("    else {{\n        body.insert(\"{}\".to_string(), serde_json::json!({}));\n    }}\n",
                        param.name,
                        match default {
                            serde_json::Value::String(s) => format!("\"{}\"", s),
                            other => other.to_string(),
                        }
                    ));
                }
            }
        }

        // Make request
        let content_type = match cmd.request.content_type {
            ContentType::Json => "json",
            ContentType::Form => "form",
            ContentType::Multipart => "multipart",
        };

        code.push_str(&format!(
            "\n    let result = do_request(ctx, \"{}\", &path, &query, &body, \"{}\").await?;\n",
            match cmd.request.method {
                HttpMethod::Get => "GET",
                HttpMethod::Post => "POST",
                HttpMethod::Put => "PUT",
                HttpMethod::Patch => "PATCH",
                HttpMethod::Delete => "DELETE",
            },
            content_type,
        ));

        // Format output
        let success_path = cmd.response.as_ref().and_then(|r| r.success_path.as_deref());
        match success_path {
            Some(path) => code.push_str(&format!("    format_output(ctx, &result, Some(\"{}\"));\n", path)),
            None => code.push_str("    format_output(ctx, &result, None);\n"),
        }

        code.push_str("    Ok(())\n}\n\n");
        Ok(())
    }
}

// Utility functions

/// Rust reserved keywords that need r# prefix when used as identifiers.
const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn",
    "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in",
    "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while", "yield",
    "abstract", "become", "box", "do", "final", "macro", "override",
    "priv", "typeof", "unsized", "virtual",
];

/// Escape a Rust keyword with r# prefix if needed.
fn escape_keyword(name: &str) -> String {
    if RUST_KEYWORDS.contains(&name) {
        format!("r#{}", name)
    } else {
        name.to_string()
    }
}

/// Convert a param name to a safe Rust identifier.
fn safe_ident(name: &str) -> String {
    let ident = name.replace('-', "_");
    escape_keyword(&ident)
}

/// Sanitize a string for use in Rust string literals and Cargo.toml.
fn sanitize_string(s: &str) -> String {
    // Take first sentence or first 200 chars, whichever is shorter
    let s = s.replace('\n', " ").replace('\r', "");
    let first_sentence = s.split_once(". ")
        .map(|(first, _)| format!("{}.", first))
        .unwrap_or_else(|| s.clone());
    let truncated = if first_sentence.len() > 200 {
        format!("{}...", &first_sentence[..197])
    } else {
        first_sentence
    };
    truncated.replace('\\', "\\\\").replace('"', "'")
}

fn to_pascal_case(s: &str) -> String {
    s.split('-')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().to_string() + &chars.as_str().to_lowercase(),
            }
        })
        .collect()
}
