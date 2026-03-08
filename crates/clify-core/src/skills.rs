//! Agent skill generator — produces SKILL.md files from a ClifySpec.
//!
//! Generates three tiers of skills:
//!   1. **Shared skill** — auth, global flags, common patterns
//!   2. **Service skills** — per-group command reference
//!   3. **Action skills** — per-command detailed usage

use crate::spec::{
    Auth, ClifySpec, Command, Group, HttpMethod, OutputFormat, Param, ParamType,
};
use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::io;
use std::path::Path;

/// Options for skill generation.
#[derive(Debug, Clone)]
pub struct SkillGenOptions {
    /// Generate action skills (per-command). If false, only shared + service skills.
    pub actions: bool,
    /// Include example commands in action skills.
    pub examples: bool,
    /// Custom category for YAML frontmatter.
    pub category: Option<String>,
}

impl Default for SkillGenOptions {
    fn default() -> Self {
        Self {
            actions: true,
            examples: true,
            category: None,
        }
    }
}

/// Result of skill generation.
pub struct SkillGenResult {
    pub skills_dir: String,
    pub shared_skill: String,
    pub service_skills: Vec<String>,
    pub action_skills: Vec<String>,
    pub total_files: usize,
}

/// Generate agent skills from a ClifySpec into the given output directory.
pub fn generate_skills(
    spec: &ClifySpec,
    output_dir: &Path,
    opts: &SkillGenOptions,
) -> Result<SkillGenResult, io::Error> {
    let skills_dir = output_dir.join("skills");
    fs::create_dir_all(&skills_dir)?;

    let cli_name = &spec.meta.name;
    let category = opts.category.as_deref().unwrap_or("api");

    // Group commands by group name
    let mut grouped: HashMap<String, Vec<&Command>> = HashMap::new();
    let mut ungrouped: Vec<&Command> = Vec::new();

    for cmd in &spec.commands {
        if let Some(ref group) = cmd.group {
            grouped.entry(group.clone()).or_default().push(cmd);
        } else {
            ungrouped.push(cmd);
        }
    }

    // Build group lookup
    let group_map: HashMap<String, &Group> = spec
        .groups
        .iter()
        .map(|g| (g.name.clone(), g))
        .collect();

    let mut service_skill_names = Vec::new();
    let mut action_skill_names = Vec::new();
    let mut total_files = 0;

    // 1. Generate shared skill
    let shared_dir = skills_dir.join(format!("{cli_name}-shared"));
    fs::create_dir_all(&shared_dir)?;
    let shared_content = gen_shared_skill(spec, category);
    fs::write(shared_dir.join("SKILL.md"), &shared_content)?;
    total_files += 1;

    // 2. Generate service skills (one per group)
    for (group_name, commands) in &grouped {
        let group_desc = group_map
            .get(group_name)
            .map(|g| g.description.as_str())
            .unwrap_or("");

        let skill_name = format!("{cli_name}-{group_name}");
        let skill_dir = skills_dir.join(&skill_name);
        fs::create_dir_all(&skill_dir)?;

        let content = gen_service_skill(
            spec, cli_name, group_name, group_desc, commands, category,
        );
        fs::write(skill_dir.join("SKILL.md"), &content)?;
        service_skill_names.push(skill_name);
        total_files += 1;
    }

    // If there are ungrouped commands, create a "general" service skill
    if !ungrouped.is_empty() {
        let skill_name = format!("{cli_name}-commands");
        let skill_dir = skills_dir.join(&skill_name);
        fs::create_dir_all(&skill_dir)?;

        let content = gen_service_skill(
            spec, cli_name, "commands", "General commands", &ungrouped, category,
        );
        fs::write(skill_dir.join("SKILL.md"), &content)?;
        service_skill_names.push(skill_name);
        total_files += 1;
    }

    // 3. Generate action skills (one per command)
    if opts.actions {
        for cmd in &spec.commands {
            let group_prefix = cmd
                .group
                .as_deref()
                .map(|g| format!("{cli_name}-{g}"))
                .unwrap_or_else(|| format!("{cli_name}-commands"));

            let skill_name = format!("{group_prefix}-{}", cmd.name);
            let skill_dir = skills_dir.join(&skill_name);
            fs::create_dir_all(&skill_dir)?;

            let content = gen_action_skill(spec, cli_name, cmd, category, opts.examples);
            fs::write(skill_dir.join("SKILL.md"), &content)?;
            action_skill_names.push(skill_name);
            total_files += 1;
        }
    }

    Ok(SkillGenResult {
        skills_dir: skills_dir.display().to_string(),
        shared_skill: format!("{cli_name}-shared"),
        service_skills: service_skill_names,
        action_skills: action_skill_names,
        total_files,
    })
}

// ── Shared Skill ────────────────────────────────────────────────────────────

fn gen_shared_skill(spec: &ClifySpec, category: &str) -> String {
    let cli = &spec.meta.name;
    let mut s = String::new();

    // Frontmatter
    writeln!(s, "---").unwrap();
    writeln!(s, "name: {cli}-shared").unwrap();
    writeln!(s, "version: {}", spec.meta.version).unwrap();
    writeln!(
        s,
        "description: \"{cli} CLI: Shared patterns for authentication, global flags, and output formatting.\""
    ).unwrap();
    writeln!(s, "metadata:").unwrap();
    writeln!(s, "  openclaw:").unwrap();
    writeln!(s, "    category: \"{}\"", category).unwrap();
    writeln!(s, "    requires:").unwrap();
    writeln!(s, "      bins: [\"{cli}\"]").unwrap();
    writeln!(s, "---").unwrap();
    writeln!(s).unwrap();

    // Title
    writeln!(s, "# {cli} — Shared Reference").unwrap();
    writeln!(s).unwrap();

    // Description
    writeln!(s, "{}", spec.meta.description).unwrap();
    writeln!(s).unwrap();

    // Base URL
    writeln!(s, "## Base URL").unwrap();
    writeln!(s).unwrap();
    writeln!(s, "```").unwrap();
    writeln!(s, "{}", spec.transport.base_url).unwrap();
    writeln!(s, "```").unwrap();
    writeln!(s).unwrap();
    writeln!(s, "Override with `--base-url <URL>`.").unwrap();
    writeln!(s).unwrap();

    // Authentication
    writeln!(s, "## Authentication").unwrap();
    writeln!(s).unwrap();
    match &spec.auth {
        Auth::None => {
            writeln!(s, "No authentication required.").unwrap();
        }
        Auth::ApiKey { location, name, env } => {
            writeln!(s, "**Type:** API Key").unwrap();
            writeln!(s, "- **Location:** {location:?}").unwrap();
            writeln!(s, "- **Header/Param:** `{name}`").unwrap();
            writeln!(s, "- **Environment variable:** `{env}`").unwrap();
            writeln!(s).unwrap();
            writeln!(s, "```bash").unwrap();
            writeln!(s, "export {env}=your-api-key").unwrap();
            writeln!(s, "{cli} auth login").unwrap();
            writeln!(s, "```").unwrap();
        }
        Auth::Token { env } => {
            writeln!(s, "**Type:** Bearer Token").unwrap();
            writeln!(s, "- **Environment variable:** `{env}`").unwrap();
            writeln!(s).unwrap();
            writeln!(s, "```bash").unwrap();
            writeln!(s, "export {env}=your-token").unwrap();
            writeln!(s, "{cli} auth login").unwrap();
            writeln!(s, "```").unwrap();
            writeln!(s).unwrap();
            writeln!(s, "Or pass inline: `{cli} --token <TOKEN> <command>`").unwrap();
        }
        Auth::Basic {
            env_user, env_pass, ..
        } => {
            writeln!(s, "**Type:** Basic Auth").unwrap();
            writeln!(s, "- **Username env:** `{env_user}`").unwrap();
            writeln!(s, "- **Password env:** `{env_pass}`").unwrap();
            writeln!(s).unwrap();
            writeln!(s, "```bash").unwrap();
            writeln!(s, "export {env_user}=your-username").unwrap();
            writeln!(s, "export {env_pass}=your-password").unwrap();
            writeln!(s, "{cli} auth login").unwrap();
            writeln!(s, "```").unwrap();
        }
        Auth::Oauth2 {
            token_url,
            scopes,
            grant,
            ..
        } => {
            writeln!(s, "**Type:** OAuth 2.0 ({grant:?})").unwrap();
            writeln!(s, "- **Token URL:** `{token_url}`").unwrap();
            if !scopes.is_empty() {
                writeln!(s, "- **Scopes:** {}", scopes.join(", ")).unwrap();
            }
            writeln!(s).unwrap();
            writeln!(s, "```bash").unwrap();
            writeln!(s, "{cli} auth login").unwrap();
            writeln!(s, "{cli} auth status  # check current auth").unwrap();
            writeln!(s, "{cli} auth logout  # clear credentials").unwrap();
            writeln!(s, "```").unwrap();
        }
    }
    writeln!(s).unwrap();

    // Global flags
    writeln!(s, "## Global Flags").unwrap();
    writeln!(s).unwrap();
    writeln!(s, "| Flag | Description |").unwrap();
    writeln!(s, "|------|-------------|").unwrap();
    writeln!(
        s,
        "| `--output <FORMAT>` | Output format: `{}` (default), {} |",
        format_output_default(&spec.output.default_format),
        format_output_others(&spec.output.default_format),
    ).unwrap();
    writeln!(s, "| `--dry-run` | Preview the HTTP request without sending |").unwrap();
    writeln!(s, "| `--verbose` | Show request/response details |").unwrap();
    writeln!(s, "| `--base-url <URL>` | Override the default server URL |").unwrap();
    writeln!(s, "| `--token <TOKEN>` | Override auth for a single request |").unwrap();
    writeln!(s).unwrap();

    // Built-in commands
    writeln!(s, "## Built-in Commands").unwrap();
    writeln!(s).unwrap();
    writeln!(s, "| Command | Description |").unwrap();
    writeln!(s, "|---------|-------------|").unwrap();
    writeln!(s, "| `{cli} auth login` | Authenticate / store credentials |").unwrap();
    writeln!(s, "| `{cli} auth status` | Check authentication state |").unwrap();
    writeln!(s, "| `{cli} auth logout` | Clear stored credentials |").unwrap();
    writeln!(s, "| `{cli} config set <key> <value>` | Set a config value |").unwrap();
    writeln!(s, "| `{cli} config get <key>` | Get a config value |").unwrap();
    writeln!(s, "| `{cli} config list` | List all config values |").unwrap();
    writeln!(s, "| `{cli} config reset` | Reset config to defaults |").unwrap();
    writeln!(s).unwrap();

    // CLI syntax
    writeln!(s, "## CLI Syntax").unwrap();
    writeln!(s).unwrap();
    if !spec.groups.is_empty() {
        writeln!(s, "```bash").unwrap();
        writeln!(s, "{cli} <group> <command> [flags]").unwrap();
        writeln!(s, "```").unwrap();
    } else {
        writeln!(s, "```bash").unwrap();
        writeln!(s, "{cli} <command> [flags]").unwrap();
        writeln!(s, "```").unwrap();
    }
    writeln!(s).unwrap();

    // Safety
    writeln!(s, "## Safety Rules").unwrap();
    writeln!(s).unwrap();
    writeln!(s, "- **Always** confirm with user before executing write/delete commands (POST, PUT, PATCH, DELETE)").unwrap();
    writeln!(s, "- Prefer `--dry-run` for destructive operations").unwrap();
    writeln!(s, "- **Never** output secrets (API keys, tokens, passwords) directly").unwrap();
    writeln!(s).unwrap();

    s
}

// ── Service Skill ───────────────────────────────────────────────────────────

fn gen_service_skill(
    spec: &ClifySpec,
    cli_name: &str,
    group_name: &str,
    group_desc: &str,
    commands: &[&Command],
    category: &str,
) -> String {
    let mut s = String::new();
    let skill_name = format!("{cli_name}-{group_name}");

    // Frontmatter
    writeln!(s, "---").unwrap();
    writeln!(s, "name: {skill_name}").unwrap();
    writeln!(s, "version: {}", spec.meta.version).unwrap();
    writeln!(
        s,
        "description: \"{}: {}\"",
        capitalize(group_name),
        sanitize_yaml_str(group_desc)
    ).unwrap();
    writeln!(s, "metadata:").unwrap();
    writeln!(s, "  openclaw:").unwrap();
    writeln!(s, "    category: \"{}\"", category).unwrap();
    writeln!(s, "    requires:").unwrap();
    writeln!(s, "      bins: [\"{cli_name}\"]").unwrap();
    writeln!(s, "    cliHelp: \"{cli_name} {group_name} --help\"").unwrap();
    writeln!(s, "---").unwrap();
    writeln!(s).unwrap();

    // Title
    writeln!(s, "# {group_name}").unwrap();
    writeln!(s).unwrap();
    writeln!(
        s,
        "> **PREREQUISITE:** Read `../{cli_name}-shared/SKILL.md` for auth, global flags, and safety rules."
    ).unwrap();
    writeln!(s).unwrap();
    if !group_desc.is_empty() {
        writeln!(s, "{group_desc}").unwrap();
        writeln!(s).unwrap();
    }

    // Commands table
    writeln!(s, "## Commands").unwrap();
    writeln!(s).unwrap();
    writeln!(s, "| Command | Method | Description |").unwrap();
    writeln!(s, "|---------|--------|-------------|").unwrap();
    for cmd in commands {
        let method = format_method(&cmd.request.method);
        let action_link = format!(
            "[`{}`](../{skill_name}-{}/SKILL.md)",
            cmd.name, cmd.name
        );
        writeln!(
            s,
            "| {} | `{}` | {} |",
            action_link, method, cmd.description
        ).unwrap();
    }
    writeln!(s).unwrap();

    // Quick reference
    writeln!(s, "## Quick Reference").unwrap();
    writeln!(s).unwrap();
    writeln!(s, "```bash").unwrap();
    writeln!(s, "# List available commands").unwrap();
    writeln!(s, "{cli_name} {group_name} --help").unwrap();
    writeln!(s).unwrap();
    // Show first command as example
    if let Some(first) = commands.first() {
        writeln!(
            s,
            "# Example: {}",
            first.description
        ).unwrap();
        writeln!(
            s,
            "{cli_name} {} {}",
            if group_name != "commands" {
                format!("{group_name} {}", first.name)
            } else {
                first.name.clone()
            },
            format_example_flags(first),
        ).unwrap();
    }
    writeln!(s, "```").unwrap();
    writeln!(s).unwrap();

    // See Also
    writeln!(s, "## See Also").unwrap();
    writeln!(s).unwrap();
    writeln!(
        s,
        "- [{cli_name}-shared](../{cli_name}-shared/SKILL.md) — Global flags and auth"
    ).unwrap();
    writeln!(s).unwrap();

    s
}

// ── Action Skill ────────────────────────────────────────────────────────────

fn gen_action_skill(
    spec: &ClifySpec,
    cli_name: &str,
    cmd: &Command,
    category: &str,
    include_examples: bool,
) -> String {
    let mut s = String::new();
    let group_prefix = cmd
        .group
        .as_deref()
        .map(|g| format!("{cli_name}-{g}"))
        .unwrap_or_else(|| format!("{cli_name}-commands"));
    let skill_name = format!("{group_prefix}-{}", cmd.name);

    let full_cmd = if let Some(ref group) = cmd.group {
        format!("{cli_name} {group} {}", cmd.name)
    } else {
        format!("{cli_name} {}", cmd.name)
    };

    // Frontmatter
    writeln!(s, "---").unwrap();
    writeln!(s, "name: {skill_name}").unwrap();
    writeln!(s, "version: {}", spec.meta.version).unwrap();
    writeln!(
        s,
        "description: \"{}\"",
        sanitize_yaml_str(&cmd.description)
    ).unwrap();
    writeln!(s, "metadata:").unwrap();
    writeln!(s, "  openclaw:").unwrap();
    writeln!(s, "    category: \"{}\"", category).unwrap();
    writeln!(s, "    requires:").unwrap();
    writeln!(s, "      bins: [\"{cli_name}\"]").unwrap();
    writeln!(s, "    cliHelp: \"{full_cmd} --help\"").unwrap();
    writeln!(s, "---").unwrap();
    writeln!(s).unwrap();

    // Title
    writeln!(s, "# {}", cmd.name).unwrap();
    writeln!(s).unwrap();
    writeln!(
        s,
        "> **PREREQUISITE:** Read `../{cli_name}-shared/SKILL.md` for auth, global flags, and safety rules."
    ).unwrap();
    writeln!(s).unwrap();

    // Description
    writeln!(s, "{}", cmd.description).unwrap();
    if let Some(ref long) = cmd.long_description {
        writeln!(s).unwrap();
        writeln!(s, "{long}").unwrap();
    }
    writeln!(s).unwrap();

    // HTTP details
    writeln!(s, "**Method:** `{} {}`", format_method(&cmd.request.method), cmd.request.path).unwrap();
    writeln!(s).unwrap();

    // Write/delete warning
    match cmd.request.method {
        HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch | HttpMethod::Delete => {
            writeln!(s, "> [!CAUTION]").unwrap();
            writeln!(
                s,
                "> This is a **write** command ({}) — confirm with the user before executing.",
                format_method(&cmd.request.method)
            ).unwrap();
            writeln!(s).unwrap();
        }
        _ => {}
    }

    // Usage
    writeln!(s, "## Usage").unwrap();
    writeln!(s).unwrap();
    writeln!(s, "```bash").unwrap();
    write!(s, "{full_cmd}").unwrap();
    for p in &cmd.params {
        if p.required {
            write!(s, " --{} <{}>", p.name, param_type_hint(p)).unwrap();
        }
    }
    let has_optional = cmd.params.iter().any(|p| !p.required);
    if has_optional {
        write!(s, " [flags]").unwrap();
    }
    writeln!(s).unwrap();
    writeln!(s, "```").unwrap();
    writeln!(s).unwrap();

    // Parameters table
    if !cmd.params.is_empty() {
        writeln!(s, "## Parameters").unwrap();
        writeln!(s).unwrap();
        writeln!(s, "| Flag | Required | Type | Default | Description |").unwrap();
        writeln!(s, "|------|----------|------|---------|-------------|").unwrap();
        for p in &cmd.params {
            if p.hidden {
                continue;
            }
            let flag = if let Some(ref short) = p.short {
                format!("`-{short}`, `--{}`", p.name)
            } else {
                format!("`--{}`", p.name)
            };
            let req = if p.required { "✓" } else { "—" };
            let ptype = format_param_type(&p.param_type);
            let default = p
                .default
                .as_ref()
                .map(|d| format!("`{d}`"))
                .unwrap_or_else(|| "—".to_string());
            let desc = &p.description;
            let values_note = if !p.values.is_empty() {
                format!(" ({})", p.values.join(", "))
            } else {
                String::new()
            };
            writeln!(
                s,
                "| {flag} | {req} | {ptype} | {default} | {desc}{values_note} |"
            ).unwrap();
        }
        writeln!(s).unwrap();
    }

    // Pagination
    if let Some(ref resp) = cmd.response {
        if let Some(ref pag) = resp.pagination {
            writeln!(s, "## Pagination").unwrap();
            writeln!(s).unwrap();
            writeln!(s, "This command supports automatic pagination.").unwrap();
            writeln!(s, "- **Type:** {:?}", pag.pagination_type).unwrap();
            writeln!(s, "- **Page param:** `--{}`", pag.param).unwrap();
            if let Some(ref ps) = pag.page_size_param {
                writeln!(s, "- **Page size param:** `--{ps}`").unwrap();
            }
            if let Some(size) = pag.default_page_size {
                writeln!(s, "- **Default page size:** {size}").unwrap();
            }
            writeln!(s).unwrap();
        }
    }

    // Examples
    if include_examples {
        let has_spec_examples = !cmd.examples.is_empty();
        if has_spec_examples {
            writeln!(s, "## Examples").unwrap();
            writeln!(s).unwrap();
            for ex in &cmd.examples {
                writeln!(s, "```bash").unwrap();
                writeln!(s, "# {}", ex.description).unwrap();
                writeln!(s, "{}", ex.command).unwrap();
                writeln!(s, "```").unwrap();
                writeln!(s).unwrap();
            }
        } else {
            // Auto-generate a basic example
            writeln!(s, "## Examples").unwrap();
            writeln!(s).unwrap();
            writeln!(s, "```bash").unwrap();
            writeln!(s, "# {}", cmd.description).unwrap();
            write!(s, "{full_cmd}").unwrap();
            for p in &cmd.params {
                if p.required {
                    write!(s, " --{} {}", p.name, param_example_value(p)).unwrap();
                }
            }
            writeln!(s).unwrap();
            writeln!(s).unwrap();
            writeln!(s, "# Dry run (preview without executing)").unwrap();
            write!(s, "{full_cmd} --dry-run").unwrap();
            for p in &cmd.params {
                if p.required {
                    write!(s, " --{} {}", p.name, param_example_value(p)).unwrap();
                }
            }
            writeln!(s).unwrap();
            writeln!(s, "```").unwrap();
            writeln!(s).unwrap();
        }
    }

    // Tips
    writeln!(s, "## Tips").unwrap();
    writeln!(s).unwrap();
    match cmd.request.method {
        HttpMethod::Get => {
            writeln!(s, "- Read-only — does not modify data.").unwrap();
        }
        _ => {
            writeln!(
                s,
                "- Use `--dry-run` to preview the request before sending."
            ).unwrap();
        }
    }
    writeln!(s, "- Use `--output table` for a quick visual overview.").unwrap();
    writeln!(s, "- Use `--verbose` to see full request/response details.").unwrap();
    writeln!(s).unwrap();

    // See Also
    writeln!(s, "## See Also").unwrap();
    writeln!(s).unwrap();
    writeln!(
        s,
        "- [{cli_name}-shared](../{cli_name}-shared/SKILL.md) — Global flags and auth"
    ).unwrap();
    writeln!(
        s,
        "- [{group_prefix}](../{group_prefix}/SKILL.md) — All {} commands",
        cmd.group.as_deref().unwrap_or("general")
    ).unwrap();
    writeln!(s).unwrap();

    s
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn format_method(m: &HttpMethod) -> &'static str {
    match m {
        HttpMethod::Get => "GET",
        HttpMethod::Post => "POST",
        HttpMethod::Put => "PUT",
        HttpMethod::Patch => "PATCH",
        HttpMethod::Delete => "DELETE",
    }
}

fn format_param_type(t: &ParamType) -> &'static str {
    match t {
        ParamType::String => "string",
        ParamType::Integer => "integer",
        ParamType::Float => "float",
        ParamType::Boolean => "boolean",
        ParamType::Enum => "enum",
        ParamType::Array => "array",
        ParamType::File => "file",
        ParamType::Object => "object",
    }
}

fn param_type_hint(p: &Param) -> String {
    if !p.values.is_empty() {
        p.values.join("|")
    } else {
        match p.param_type {
            ParamType::String => "VALUE".to_string(),
            ParamType::Integer => "N".to_string(),
            ParamType::Float => "N.N".to_string(),
            ParamType::Boolean => "true|false".to_string(),
            ParamType::Enum => "VALUE".to_string(),
            ParamType::Array => "VAL1,VAL2".to_string(),
            ParamType::File => "PATH".to_string(),
            ParamType::Object => "JSON".to_string(),
        }
    }
}

fn param_example_value(p: &Param) -> String {
    if let Some(ref d) = p.default {
        return d.to_string().trim_matches('"').to_string();
    }
    if !p.values.is_empty() {
        return p.values[0].clone();
    }
    match p.param_type {
        ParamType::String => "<value>".to_string(),
        ParamType::Integer => "10".to_string(),
        ParamType::Float => "1.0".to_string(),
        ParamType::Boolean => "true".to_string(),
        ParamType::Enum => "<value>".to_string(),
        ParamType::Array => "val1,val2".to_string(),
        ParamType::File => "./file.txt".to_string(),
        ParamType::Object => "'{}'".to_string(),
    }
}

fn format_output_default(f: &OutputFormat) -> &'static str {
    match f {
        OutputFormat::Json => "json",
        OutputFormat::Table => "table",
        OutputFormat::Csv => "csv",
    }
}

fn format_output_others(default: &OutputFormat) -> String {
    let all = ["json", "table", "csv"];
    let default_str = format_output_default(default);
    let others: Vec<&str> = all.iter().copied().filter(|x| *x != default_str).collect();
    others
        .iter()
        .map(|x| format!("`{x}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_example_flags(cmd: &Command) -> String {
    let mut flags = String::new();
    for p in &cmd.params {
        if p.required {
            write!(flags, "--{} {} ", p.name, param_example_value(p)).unwrap();
        }
    }
    flags.trim().to_string()
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().to_string() + c.as_str(),
    }
}

fn sanitize_yaml_str(s: &str) -> String {
    s.replace('"', "'").replace('\n', " ")
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::*;
    use tempfile::TempDir;

    fn make_test_spec() -> ClifySpec {
        ClifySpec {
            meta: Meta {
                name: "myapi".to_string(),
                version: "1.0.0".to_string(),
                description: "My test API".to_string(),
                long_description: None,
                author: None,
                license: None,
                homepage: None,
            },
            transport: Transport {
                transport_type: TransportType::Rest,
                base_url: "https://api.example.com/v1".to_string(),
                timeout: 30,
                retries: 0,
                headers: HashMap::new(),
            },
            auth: Auth::Token {
                env: "MY_API_TOKEN".to_string(),
            },
            output: Output::default(),
            config: Config::default(),
            groups: vec![Group {
                name: "users".to_string(),
                description: "User management operations".to_string(),
            }],
            commands: vec![
                Command {
                    name: "list-users".to_string(),
                    description: "List all users".to_string(),
                    long_description: None,
                    group: Some("users".to_string()),
                    aliases: vec![],
                    hidden: false,
                    request: Request {
                        method: HttpMethod::Get,
                        path: "/users".to_string(),
                        content_type: ContentType::Json,
                        headers: HashMap::new(),
                    },
                    params: vec![Param {
                        name: "limit".to_string(),
                        param_type: ParamType::Integer,
                        required: false,
                        description: "Maximum results to return".to_string(),
                        short: Some("l".to_string()),
                        default: Some(serde_json::json!(25)),
                        env: None,
                        source: Some(ParamSource::Query),
                        hidden: false,
                        values: vec![],
                        separator: None,
                        file_type: None,
                        mime_type: None,
                        validation: None,
                    }],
                    response: None,
                    examples: vec![],
                    hooks: None,
                },
                Command {
                    name: "create-user".to_string(),
                    description: "Create a new user".to_string(),
                    long_description: None,
                    group: Some("users".to_string()),
                    aliases: vec![],
                    hidden: false,
                    request: Request {
                        method: HttpMethod::Post,
                        path: "/users".to_string(),
                        content_type: ContentType::Json,
                        headers: HashMap::new(),
                    },
                    params: vec![
                        Param {
                            name: "name".to_string(),
                            param_type: ParamType::String,
                            required: true,
                            description: "User's full name".to_string(),
                            short: None,
                            default: None,
                            env: None,
                            source: Some(ParamSource::Body),
                            hidden: false,
                            values: vec![],
                            separator: None,
                            file_type: None,
                            mime_type: None,
                            validation: None,
                        },
                        Param {
                            name: "email".to_string(),
                            param_type: ParamType::String,
                            required: true,
                            description: "User's email address".to_string(),
                            short: None,
                            default: None,
                            env: None,
                            source: Some(ParamSource::Body),
                            hidden: false,
                            values: vec![],
                            separator: None,
                            file_type: None,
                            mime_type: None,
                            validation: None,
                        },
                    ],
                    response: None,
                    examples: vec![Example {
                        description: "Create a user".to_string(),
                        command: "myapi users create-user --name \"John Doe\" --email john@example.com".to_string(),
                    }],
                    hooks: None,
                },
            ],
            hooks: None,
        }
    }

    #[test]
    fn test_generate_skills_creates_directory_structure() {
        let spec = make_test_spec();
        let tmp = TempDir::new().unwrap();
        let opts = SkillGenOptions::default();

        let result = generate_skills(&spec, tmp.path(), &opts).unwrap();

        assert!(tmp.path().join("skills/myapi-shared/SKILL.md").exists());
        assert!(tmp.path().join("skills/myapi-users/SKILL.md").exists());
        assert!(tmp
            .path()
            .join("skills/myapi-users-list-users/SKILL.md")
            .exists());
        assert!(tmp
            .path()
            .join("skills/myapi-users-create-user/SKILL.md")
            .exists());
        assert_eq!(result.total_files, 4); // shared + 1 service + 2 actions
    }

    #[test]
    fn test_shared_skill_contains_auth() {
        let spec = make_test_spec();
        let tmp = TempDir::new().unwrap();
        let opts = SkillGenOptions::default();

        generate_skills(&spec, tmp.path(), &opts).unwrap();

        let content = fs::read_to_string(
            tmp.path().join("skills/myapi-shared/SKILL.md"),
        ).unwrap();
        assert!(content.contains("Bearer Token"));
        assert!(content.contains("MY_API_TOKEN"));
        assert!(content.contains("## Global Flags"));
    }

    #[test]
    fn test_service_skill_lists_commands() {
        let spec = make_test_spec();
        let tmp = TempDir::new().unwrap();
        let opts = SkillGenOptions::default();

        generate_skills(&spec, tmp.path(), &opts).unwrap();

        let content = fs::read_to_string(
            tmp.path().join("skills/myapi-users/SKILL.md"),
        ).unwrap();
        assert!(content.contains("list-users"));
        assert!(content.contains("create-user"));
        assert!(content.contains("`GET`"));
        assert!(content.contains("`POST`"));
    }

    #[test]
    fn test_action_skill_has_params_and_caution() {
        let spec = make_test_spec();
        let tmp = TempDir::new().unwrap();
        let opts = SkillGenOptions::default();

        generate_skills(&spec, tmp.path(), &opts).unwrap();

        let content = fs::read_to_string(
            tmp.path()
                .join("skills/myapi-users-create-user/SKILL.md"),
        ).unwrap();
        assert!(content.contains("## Parameters"));
        assert!(content.contains("`--name`"));
        assert!(content.contains("`--email`"));
        assert!(content.contains("[!CAUTION]"));
        assert!(content.contains("**write** command"));
    }

    #[test]
    fn test_action_skill_read_only_no_caution() {
        let spec = make_test_spec();
        let tmp = TempDir::new().unwrap();
        let opts = SkillGenOptions::default();

        generate_skills(&spec, tmp.path(), &opts).unwrap();

        let content = fs::read_to_string(
            tmp.path()
                .join("skills/myapi-users-list-users/SKILL.md"),
        ).unwrap();
        assert!(!content.contains("[!CAUTION]"));
        assert!(content.contains("Read-only"));
    }

    #[test]
    fn test_no_actions_option() {
        let spec = make_test_spec();
        let tmp = TempDir::new().unwrap();
        let opts = SkillGenOptions {
            actions: false,
            ..Default::default()
        };

        let result = generate_skills(&spec, tmp.path(), &opts).unwrap();
        assert_eq!(result.total_files, 2); // shared + 1 service only
        assert!(result.action_skills.is_empty());
    }
}
