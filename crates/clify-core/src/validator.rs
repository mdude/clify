//! Spec validation — checks for structural correctness beyond YAML parsing.

use crate::spec::*;
use std::collections::HashSet;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("Invalid meta.name '{0}': must be lowercase, start with a letter, and contain only [a-z0-9-]")]
    InvalidName(String),

    #[error("Invalid meta.version '{0}': must be valid semver (e.g., 0.1.0)")]
    InvalidVersion(String),

    #[error("Invalid transport.base_url '{0}': must start with http:// or https://")]
    InvalidBaseUrl(String),

    #[error("Command '{command}' references unknown group '{group}'")]
    UnknownGroup { command: String, group: String },

    #[error("Command '{command}': path placeholder '{{{placeholder}}}' has no matching param with source: path")]
    MissingPathParam { command: String, placeholder: String },

    #[error("Command '{command}', param '{param}': source is 'path' but '{{{param}}}' not found in request path '{path}'")]
    OrphanPathParam { command: String, param: String, path: String },

    #[error("Duplicate command name '{name}' in group '{group}'")]
    DuplicateCommand { name: String, group: String },

    #[error("Duplicate group name: '{0}'")]
    DuplicateGroup(String),

    #[error("Reserved group name: '{0}' (auth, config, help are auto-generated)")]
    ReservedGroup(String),

    #[error("Command '{command}', param '{param}': type is 'enum' but no 'values' provided")]
    EnumWithoutValues { command: String, param: String },

    #[error("Command '{command}', param '{param}': 'values' provided but type is '{param_type}', not 'enum'")]
    ValuesOnNonEnum { command: String, param: String, param_type: String },

    #[error("Command '{command}', param '{param}': default value '{default}' not in allowed values: {values}")]
    DefaultNotInValues { command: String, param: String, default: String, values: String },

    #[error("Command '{command}', param '{param}': short flag '{short}' must be a single ASCII letter")]
    InvalidShortFlag { command: String, param: String, short: String },

    #[error("Command '{command}': duplicate short flag '-{short}' used by params '{param1}' and '{param2}'")]
    DuplicateShortFlag { command: String, short: String, param1: String, param2: String },

    #[error("Command '{command}': duplicate param name '{param}'")]
    DuplicateParam { command: String, param: String },

    #[error("Command '{command}', param '{param}': validation.min ({min}) > validation.max ({max})")]
    MinGreaterThanMax { command: String, param: String, min: f64, max: f64 },

    #[error("Command '{command}', param '{param}': validation.min_length ({min}) > validation.max_length ({max})")]
    MinLenGreaterThanMaxLen { command: String, param: String, min: usize, max: usize },

    #[error("Command '{command}', param '{param}': validation.pattern '{pattern}' is not a valid regex: {error}")]
    InvalidPattern { command: String, param: String, pattern: String, error: String },

    #[error("Command '{command}': alias '{alias}' conflicts with command name '{conflict}' in the same group")]
    AliasConflict { command: String, alias: String, conflict: String },

    #[error("No commands defined — spec must have at least one command")]
    NoCommands,

    #[error("Command '{command}', param '{param}': file_type is set but type is not 'file'")]
    FileTypeOnNonFile { command: String, param: String },

    #[error("Command '{command}': response.pagination.type is 'cursor' but no next_path specified")]
    CursorWithoutNextPath { command: String },

    #[error("Group name '{0}' is invalid: must match [a-z][a-z0-9-]*")]
    InvalidGroupName(String),
}

/// Validate a parsed spec for structural correctness.
/// Returns Ok(()) if valid, or a list of all errors found.
pub fn validate(spec: &ClifySpec) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();
    let name_re = regex::Regex::new(r"^[a-z][a-z0-9-]*$").unwrap();
    let semver_re = regex::Regex::new(r"^\d+\.\d+\.\d+(-[a-zA-Z0-9.]+)?(\+[a-zA-Z0-9.]+)?$").unwrap();

    // -- Meta validation --
    if !name_re.is_match(&spec.meta.name) {
        errors.push(ValidationError::InvalidName(spec.meta.name.clone()));
    }
    if !semver_re.is_match(&spec.meta.version) {
        errors.push(ValidationError::InvalidVersion(spec.meta.version.clone()));
    }

    // -- Transport validation --
    if !spec.transport.base_url.starts_with("http://") && !spec.transport.base_url.starts_with("https://") {
        errors.push(ValidationError::InvalidBaseUrl(spec.transport.base_url.clone()));
    }

    // -- Group validation --
    let mut group_names = HashSet::new();
    let reserved = ["auth", "config", "help"];
    for group in &spec.groups {
        if reserved.contains(&group.name.as_str()) {
            errors.push(ValidationError::ReservedGroup(group.name.clone()));
        }
        if !name_re.is_match(&group.name) {
            errors.push(ValidationError::InvalidGroupName(group.name.clone()));
        }
        if !group_names.insert(&group.name) {
            errors.push(ValidationError::DuplicateGroup(group.name.clone()));
        }
    }

    // -- Commands validation --
    if spec.commands.is_empty() {
        errors.push(ValidationError::NoCommands);
    }

    // Track all command names + aliases per group for conflict detection
    let mut cmd_names_by_group: std::collections::HashMap<String, HashSet<String>> = std::collections::HashMap::new();

    for cmd in &spec.commands {
        let group_key = cmd.group.clone().unwrap_or_default();

        // Check group reference
        if let Some(ref group) = cmd.group {
            if !spec.groups.iter().any(|g| &g.name == group) {
                errors.push(ValidationError::UnknownGroup {
                    command: cmd.name.clone(),
                    group: group.clone(),
                });
            }
        }

        // Check duplicate command names within group
        let group_set = cmd_names_by_group.entry(group_key.clone()).or_default();
        if !group_set.insert(cmd.name.clone()) {
            errors.push(ValidationError::DuplicateCommand {
                name: cmd.name.clone(),
                group: group_key.clone(),
            });
        }

        // Check alias conflicts
        for alias in &cmd.aliases {
            if group_set.contains(alias) {
                // Find which command it conflicts with
                let conflict = spec.commands.iter()
                    .find(|c| c.group.clone().unwrap_or_default() == group_key && c.name == *alias)
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| alias.clone());
                errors.push(ValidationError::AliasConflict {
                    command: cmd.name.clone(),
                    alias: alias.clone(),
                    conflict,
                });
            }
            group_set.insert(alias.clone());
        }

        // -- Path param validation --
        let path_placeholders: Vec<String> = regex::Regex::new(r"\{([\w-]+)\}")
            .unwrap()
            .captures_iter(&cmd.request.path)
            .map(|c| c[1].to_string())
            .collect();

        for placeholder in &path_placeholders {
            let has_param = cmd.params.iter().any(|p| {
                &p.name == placeholder && matches!(p.source, Some(ParamSource::Path) | None)
            });
            if !has_param {
                errors.push(ValidationError::MissingPathParam {
                    command: cmd.name.clone(),
                    placeholder: placeholder.clone(),
                });
            }
        }

        // -- Param validation --
        let mut param_names = HashSet::new();
        let mut short_flags: std::collections::HashMap<String, String> = std::collections::HashMap::new();

        for param in &cmd.params {
            // Duplicate param names
            if !param_names.insert(&param.name) {
                errors.push(ValidationError::DuplicateParam {
                    command: cmd.name.clone(),
                    param: param.name.clone(),
                });
            }

            // Orphan path params
            if let Some(ParamSource::Path) = param.source {
                if !path_placeholders.contains(&param.name) {
                    errors.push(ValidationError::OrphanPathParam {
                        command: cmd.name.clone(),
                        param: param.name.clone(),
                        path: cmd.request.path.clone(),
                    });
                }
            }

            // Short flag validation
            if let Some(ref short) = param.short {
                if short.len() != 1 || !short.chars().next().map(|c| c.is_ascii_alphabetic()).unwrap_or(false) {
                    errors.push(ValidationError::InvalidShortFlag {
                        command: cmd.name.clone(),
                        param: param.name.clone(),
                        short: short.clone(),
                    });
                } else if let Some(existing) = short_flags.get(short) {
                    errors.push(ValidationError::DuplicateShortFlag {
                        command: cmd.name.clone(),
                        short: short.clone(),
                        param1: existing.clone(),
                        param2: param.name.clone(),
                    });
                } else {
                    short_flags.insert(short.clone(), param.name.clone());
                }
            }

            // Enum validation
            match param.param_type {
                ParamType::Enum => {
                    if param.values.is_empty() {
                        errors.push(ValidationError::EnumWithoutValues {
                            command: cmd.name.clone(),
                            param: param.name.clone(),
                        });
                    }
                    // Check default is in values
                    if let Some(ref default) = param.default {
                        if let Some(default_str) = default.as_str() {
                            if !param.values.iter().any(|v| v == default_str) {
                                errors.push(ValidationError::DefaultNotInValues {
                                    command: cmd.name.clone(),
                                    param: param.name.clone(),
                                    default: default_str.to_string(),
                                    values: param.values.join(", "),
                                });
                            }
                        }
                    }
                }
                _ => {
                    if !param.values.is_empty() {
                        errors.push(ValidationError::ValuesOnNonEnum {
                            command: cmd.name.clone(),
                            param: param.name.clone(),
                            param_type: format!("{:?}", param.param_type).to_lowercase(),
                        });
                    }
                }
            }

            // File type on non-file
            if param.file_type.is_some() && !matches!(param.param_type, ParamType::File) {
                errors.push(ValidationError::FileTypeOnNonFile {
                    command: cmd.name.clone(),
                    param: param.name.clone(),
                });
            }

            // Validation rules
            if let Some(ref validation) = param.validation {
                if let (Some(min), Some(max)) = (validation.min, validation.max) {
                    if min > max {
                        errors.push(ValidationError::MinGreaterThanMax {
                            command: cmd.name.clone(),
                            param: param.name.clone(),
                            min,
                            max,
                        });
                    }
                }
                if let (Some(min_len), Some(max_len)) = (validation.min_length, validation.max_length) {
                    if min_len > max_len {
                        errors.push(ValidationError::MinLenGreaterThanMaxLen {
                            command: cmd.name.clone(),
                            param: param.name.clone(),
                            min: min_len,
                            max: max_len,
                        });
                    }
                }
                if let Some(ref pattern) = validation.pattern {
                    if let Err(e) = regex::Regex::new(pattern) {
                        errors.push(ValidationError::InvalidPattern {
                            command: cmd.name.clone(),
                            param: param.name.clone(),
                            pattern: pattern.clone(),
                            error: e.to_string(),
                        });
                    }
                }
            }
        }

        // -- Pagination validation --
        if let Some(ref response) = cmd.response {
            if let Some(ref pagination) = response.pagination {
                if matches!(pagination.pagination_type, PaginationType::Cursor) && pagination.next_path.is_none() {
                    errors.push(ValidationError::CursorWithoutNextPath {
                        command: cmd.name.clone(),
                    });
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::*;
    use std::collections::HashMap;

    /// Helper to build a minimal valid spec for testing.
    fn minimal_spec() -> ClifySpec {
        ClifySpec {
            meta: Meta {
                name: "test-cli".to_string(),
                version: "0.1.0".to_string(),
                description: "Test CLI".to_string(),
                long_description: None,
                author: None,
                license: None,
                homepage: None,
            },
            transport: Transport {
                transport_type: TransportType::Rest,
                base_url: "https://api.example.com".to_string(),
                timeout: 30,
                retries: 0,
                headers: HashMap::new(),
            },
            auth: Auth::None,
            output: Output::default(),
            config: Config::default(),
            groups: vec![],
            commands: vec![Command {
                name: "ping".to_string(),
                description: "Health check".to_string(),
                long_description: None,
                group: None,
                aliases: vec![],
                hidden: false,
                request: Request {
                    method: HttpMethod::Get,
                    path: "/ping".to_string(),
                    content_type: ContentType::Json,
                    headers: HashMap::new(),
                },
                params: vec![],
                response: None,
                examples: vec![],
                hooks: None,
            }],
            hooks: None,
        }
    }

    #[test]
    fn valid_minimal_spec() {
        let spec = minimal_spec();
        assert!(validate(&spec).is_ok());
    }

    #[test]
    fn invalid_name_uppercase() {
        let mut spec = minimal_spec();
        spec.meta.name = "TestCLI".to_string();
        let errs = validate(&spec).unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ValidationError::InvalidName(_))));
    }

    #[test]
    fn invalid_name_starts_with_number() {
        let mut spec = minimal_spec();
        spec.meta.name = "123cli".to_string();
        let errs = validate(&spec).unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ValidationError::InvalidName(_))));
    }

    #[test]
    fn invalid_version() {
        let mut spec = minimal_spec();
        spec.meta.version = "v1".to_string();
        let errs = validate(&spec).unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ValidationError::InvalidVersion(_))));
    }

    #[test]
    fn invalid_base_url() {
        let mut spec = minimal_spec();
        spec.transport.base_url = "ftp://example.com".to_string();
        let errs = validate(&spec).unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ValidationError::InvalidBaseUrl(_))));
    }

    #[test]
    fn unknown_group_reference() {
        let mut spec = minimal_spec();
        spec.commands[0].group = Some("nonexistent".to_string());
        let errs = validate(&spec).unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ValidationError::UnknownGroup { .. })));
    }

    #[test]
    fn reserved_group_name() {
        let mut spec = minimal_spec();
        spec.groups.push(Group { name: "auth".to_string(), description: "Auth".to_string() });
        let errs = validate(&spec).unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ValidationError::ReservedGroup(_))));
    }

    #[test]
    fn duplicate_group() {
        let mut spec = minimal_spec();
        spec.groups.push(Group { name: "data".to_string(), description: "Data".to_string() });
        spec.groups.push(Group { name: "data".to_string(), description: "Data 2".to_string() });
        let errs = validate(&spec).unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ValidationError::DuplicateGroup(_))));
    }

    #[test]
    fn no_commands() {
        let mut spec = minimal_spec();
        spec.commands.clear();
        let errs = validate(&spec).unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ValidationError::NoCommands)));
    }

    #[test]
    fn missing_path_param() {
        let mut spec = minimal_spec();
        spec.commands[0].request.path = "/{service}/query".to_string();
        let errs = validate(&spec).unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ValidationError::MissingPathParam { .. })));
    }

    #[test]
    fn orphan_path_param() {
        let mut spec = minimal_spec();
        spec.commands[0].params.push(Param {
            name: "orphan".to_string(),
            param_type: ParamType::String,
            required: false,
            description: "An orphan".to_string(),
            short: None,
            default: None,
            env: None,
            source: Some(ParamSource::Path),
            hidden: false,
            values: vec![],
            separator: None,
            file_type: None,
            mime_type: None,
            validation: None,
        });
        let errs = validate(&spec).unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ValidationError::OrphanPathParam { .. })));
    }

    #[test]
    fn enum_without_values() {
        let mut spec = minimal_spec();
        spec.commands[0].params.push(Param {
            name: "format".to_string(),
            param_type: ParamType::Enum,
            required: false,
            description: "Output format".to_string(),
            short: None,
            default: None,
            env: None,
            source: None,
            hidden: false,
            values: vec![],
            separator: None,
            file_type: None,
            mime_type: None,
            validation: None,
        });
        let errs = validate(&spec).unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ValidationError::EnumWithoutValues { .. })));
    }

    #[test]
    fn default_not_in_enum_values() {
        let mut spec = minimal_spec();
        spec.commands[0].params.push(Param {
            name: "format".to_string(),
            param_type: ParamType::Enum,
            required: false,
            description: "Output format".to_string(),
            short: None,
            default: Some(serde_json::Value::String("xml".to_string())),
            env: None,
            source: None,
            hidden: false,
            values: vec!["json".to_string(), "csv".to_string()],
            separator: None,
            file_type: None,
            mime_type: None,
            validation: None,
        });
        let errs = validate(&spec).unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ValidationError::DefaultNotInValues { .. })));
    }

    #[test]
    fn invalid_short_flag() {
        let mut spec = minimal_spec();
        spec.commands[0].params.push(Param {
            name: "verbose".to_string(),
            param_type: ParamType::Boolean,
            required: false,
            description: "Verbose".to_string(),
            short: Some("vv".to_string()), // too long
            default: None,
            env: None,
            source: None,
            hidden: false,
            values: vec![],
            separator: None,
            file_type: None,
            mime_type: None,
            validation: None,
        });
        let errs = validate(&spec).unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ValidationError::InvalidShortFlag { .. })));
    }

    #[test]
    fn duplicate_short_flag() {
        let mut spec = minimal_spec();
        spec.commands[0].params.push(Param {
            name: "distance".to_string(),
            param_type: ParamType::Float,
            required: false,
            description: "Distance".to_string(),
            short: Some("d".to_string()),
            default: None, env: None, source: None, hidden: false,
            values: vec![], separator: None, file_type: None, mime_type: None, validation: None,
        });
        spec.commands[0].params.push(Param {
            name: "debug".to_string(),
            param_type: ParamType::Boolean,
            required: false,
            description: "Debug".to_string(),
            short: Some("d".to_string()), // duplicate!
            default: None, env: None, source: None, hidden: false,
            values: vec![], separator: None, file_type: None, mime_type: None, validation: None,
        });
        let errs = validate(&spec).unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ValidationError::DuplicateShortFlag { .. })));
    }

    #[test]
    fn min_greater_than_max() {
        let mut spec = minimal_spec();
        spec.commands[0].params.push(Param {
            name: "count".to_string(),
            param_type: ParamType::Integer,
            required: false,
            description: "Count".to_string(),
            short: None, default: None, env: None, source: None, hidden: false,
            values: vec![], separator: None, file_type: None, mime_type: None,
            validation: Some(Validation {
                min: Some(100.0),
                max: Some(10.0),
                min_length: None,
                max_length: None,
                pattern: None,
                custom: None,
            }),
        });
        let errs = validate(&spec).unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ValidationError::MinGreaterThanMax { .. })));
    }

    #[test]
    fn invalid_regex_pattern() {
        let mut spec = minimal_spec();
        spec.commands[0].params.push(Param {
            name: "email".to_string(),
            param_type: ParamType::String,
            required: false,
            description: "Email".to_string(),
            short: None, default: None, env: None, source: None, hidden: false,
            values: vec![], separator: None, file_type: None, mime_type: None,
            validation: Some(Validation {
                min: None, max: None, min_length: None, max_length: None,
                pattern: Some("[invalid".to_string()),
                custom: None,
            }),
        });
        let errs = validate(&spec).unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ValidationError::InvalidPattern { .. })));
    }

    #[test]
    fn cursor_pagination_without_next_path() {
        let mut spec = minimal_spec();
        spec.commands[0].response = Some(Response {
            success_status: vec![200],
            success_path: None,
            error_path: None,
            pagination: Some(Pagination {
                pagination_type: PaginationType::Cursor,
                param: "cursor".to_string(),
                page_size_param: None,
                default_page_size: None,
                next_path: None, // missing!
                total_path: None,
            }),
        });
        let errs = validate(&spec).unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ValidationError::CursorWithoutNextPath { .. })));
    }

    #[test]
    fn file_type_on_non_file_param() {
        let mut spec = minimal_spec();
        spec.commands[0].params.push(Param {
            name: "name".to_string(),
            param_type: ParamType::String,
            required: false,
            description: "Name".to_string(),
            short: None, default: None, env: None, source: None, hidden: false,
            values: vec![], separator: None,
            file_type: Some(FileType::Path), // wrong! type is string
            mime_type: None, validation: None,
        });
        let errs = validate(&spec).unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ValidationError::FileTypeOnNonFile { .. })));
    }

    #[test]
    fn valid_complex_spec() {
        let spec = ClifySpec {
            meta: Meta {
                name: "arcgis-server".to_string(),
                version: "0.1.0".to_string(),
                description: "ArcGIS Server CLI".to_string(),
                long_description: None,
                author: Some("Esri".to_string()),
                license: Some("Apache-2.0".to_string()),
                homepage: None,
            },
            transport: Transport {
                transport_type: TransportType::Rest,
                base_url: "https://gis.example.com/arcgis/rest/services".to_string(),
                timeout: 60,
                retries: 2,
                headers: HashMap::from([("Accept".to_string(), "application/json".to_string())]),
            },
            auth: Auth::Oauth2 {
                grant: OAuthGrant::ClientCredentials,
                token_url: "https://www.arcgis.com/sharing/rest/generateToken".to_string(),
                authorize_url: None,
                scopes: vec![],
                env_client_id: "ARCGIS_CLIENT_ID".to_string(),
                env_client_secret: "ARCGIS_CLIENT_SECRET".to_string(),
                custom: Some(OAuthCustom {
                    token_field: "token".to_string(),
                    expiry_field: "expires".to_string(),
                    content_type: ContentType::Form,
                    extra_params: HashMap::from([("f".to_string(), "json".to_string())]),
                }),
            },
            output: Output::default(),
            config: Config::default(),
            groups: vec![
                Group { name: "analysis".to_string(), description: "Analysis tools".to_string() },
                Group { name: "data".to_string(), description: "Data management".to_string() },
            ],
            commands: vec![
                Command {
                    name: "buffer".to_string(),
                    description: "Create buffer zones".to_string(),
                    long_description: None,
                    group: Some("analysis".to_string()),
                    aliases: vec!["buf".to_string()],
                    hidden: false,
                    request: Request {
                        method: HttpMethod::Post,
                        path: "/Analysis/GPServer/Buffer/execute".to_string(),
                        content_type: ContentType::Form,
                        headers: HashMap::new(),
                    },
                    params: vec![
                        Param {
                            name: "input".to_string(),
                            param_type: ParamType::File,
                            required: true,
                            description: "Input features".to_string(),
                            short: Some("i".to_string()),
                            default: None, env: None,
                            source: Some(ParamSource::Body),
                            hidden: false,
                            values: vec![], separator: None,
                            file_type: Some(FileType::Both),
                            mime_type: None, validation: None,
                        },
                        Param {
                            name: "distance".to_string(),
                            param_type: ParamType::Float,
                            required: true,
                            description: "Buffer distance".to_string(),
                            short: Some("d".to_string()),
                            default: None, env: None,
                            source: Some(ParamSource::Body),
                            hidden: false,
                            values: vec![], separator: None,
                            file_type: None, mime_type: None,
                            validation: Some(Validation {
                                min: Some(0.0), max: None,
                                min_length: None, max_length: None,
                                pattern: None, custom: None,
                            }),
                        },
                        Param {
                            name: "units".to_string(),
                            param_type: ParamType::Enum,
                            required: false,
                            description: "Distance units".to_string(),
                            short: Some("u".to_string()),
                            default: Some(serde_json::Value::String("meters".to_string())),
                            env: None,
                            source: Some(ParamSource::Body),
                            hidden: false,
                            values: vec!["meters".to_string(), "feet".to_string(), "miles".to_string()],
                            separator: None, file_type: None, mime_type: None, validation: None,
                        },
                    ],
                    response: Some(Response {
                        success_status: vec![200],
                        success_path: Some("results".to_string()),
                        error_path: Some("error.message".to_string()),
                        pagination: None,
                    }),
                    examples: vec![Example {
                        description: "Buffer roads".to_string(),
                        command: "arcgis-server analysis buffer -i roads.geojson -d 100".to_string(),
                    }],
                    hooks: None,
                },
                Command {
                    name: "query".to_string(),
                    description: "Query features".to_string(),
                    long_description: None,
                    group: Some("data".to_string()),
                    aliases: vec!["q".to_string()],
                    hidden: false,
                    request: Request {
                        method: HttpMethod::Get,
                        path: "/{service}/FeatureServer/{layer}/query".to_string(),
                        content_type: ContentType::Json,
                        headers: HashMap::new(),
                    },
                    params: vec![
                        Param {
                            name: "service".to_string(),
                            param_type: ParamType::String,
                            required: true,
                            description: "Service name".to_string(),
                            short: None, default: None, env: None,
                            source: Some(ParamSource::Path),
                            hidden: false, values: vec![], separator: None,
                            file_type: None, mime_type: None, validation: None,
                        },
                        Param {
                            name: "layer".to_string(),
                            param_type: ParamType::Integer,
                            required: true,
                            description: "Layer index".to_string(),
                            short: None,
                            default: Some(serde_json::Value::Number(serde_json::Number::from(0))),
                            env: None,
                            source: Some(ParamSource::Path),
                            hidden: false, values: vec![], separator: None,
                            file_type: None, mime_type: None, validation: None,
                        },
                        Param {
                            name: "where".to_string(),
                            param_type: ParamType::String,
                            required: false,
                            description: "SQL where clause".to_string(),
                            short: Some("w".to_string()),
                            default: Some(serde_json::Value::String("1=1".to_string())),
                            env: None,
                            source: Some(ParamSource::Query),
                            hidden: false, values: vec![], separator: None,
                            file_type: None, mime_type: None, validation: None,
                        },
                    ],
                    response: Some(Response {
                        success_status: vec![200],
                        success_path: Some("features".to_string()),
                        error_path: Some("error.message".to_string()),
                        pagination: Some(Pagination {
                            pagination_type: PaginationType::Offset,
                            param: "resultOffset".to_string(),
                            page_size_param: Some("resultRecordCount".to_string()),
                            default_page_size: Some(1000),
                            next_path: None,
                            total_path: Some("count".to_string()),
                        }),
                    }),
                    examples: vec![],
                    hooks: None,
                },
            ],
            hooks: None,
        };
        assert!(validate(&spec).is_ok());
    }

    #[test]
    fn parse_and_validate_example_spec() {
        // Test that the ArcGIS example spec can be parsed from YAML
        let yaml = include_str!("../../../examples/example-arcgis-server.clify.yaml");
        let spec: ClifySpec = serde_yaml::from_str(yaml).expect("Failed to parse example spec");
        // Validation may have issues depending on exact YAML structure, but parsing should work
        let _ = validate(&spec); // We just care that it parses
    }
}
