//! Spec validation — checks for structural correctness beyond YAML parsing.

use crate::spec::ClifySpec;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("Invalid meta.name '{0}': must be lowercase alphanumeric with hyphens")]
    InvalidName(String),
    #[error("Command '{command}' references unknown group '{group}'")]
    UnknownGroup { command: String, group: String },
    #[error("Command '{command}': path param '{param}' has no matching {{placeholder}} in request path")]
    OrphanPathParam { command: String, param: String },
    #[error("Command '{command}': path placeholder '{{{placeholder}}}' has no matching param")]
    MissingPathParam { command: String, placeholder: String },
    #[error("Duplicate command name: '{0}'")]
    DuplicateCommand(String),
    #[error("Duplicate group name: '{0}'")]
    DuplicateGroup(String),
    #[error("Reserved group name: '{0}'")]
    ReservedGroup(String),
}

/// Validate a parsed spec for structural correctness.
pub fn validate(spec: &ClifySpec) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();

    // Validate meta.name
    let name_re = regex::Regex::new(r"^[a-z][a-z0-9-]*$").unwrap();
    if !name_re.is_match(&spec.meta.name) {
        errors.push(ValidationError::InvalidName(spec.meta.name.clone()));
    }

    // Check for duplicate groups
    let mut group_names = std::collections::HashSet::new();
    let reserved = ["auth", "config", "help"];
    for group in &spec.groups {
        if reserved.contains(&group.name.as_str()) {
            errors.push(ValidationError::ReservedGroup(group.name.clone()));
        }
        if !group_names.insert(&group.name) {
            errors.push(ValidationError::DuplicateGroup(group.name.clone()));
        }
    }

    // Check for duplicate commands
    let mut cmd_names = std::collections::HashSet::new();
    for cmd in &spec.commands {
        if !cmd_names.insert((&cmd.group, &cmd.name)) {
            errors.push(ValidationError::DuplicateCommand(cmd.name.clone()));
        }

        // Check group references
        if let Some(ref group) = cmd.group {
            if !spec.groups.iter().any(|g| &g.name == group) {
                errors.push(ValidationError::UnknownGroup {
                    command: cmd.name.clone(),
                    group: group.clone(),
                });
            }
        }

        // Check path param consistency
        let path_placeholders: Vec<String> = regex::Regex::new(r"\{(\w+)\}")
            .unwrap()
            .captures_iter(&cmd.request.path)
            .map(|c| c[1].to_string())
            .collect();

        for placeholder in &path_placeholders {
            if !cmd.params.iter().any(|p| &p.name == placeholder) {
                errors.push(ValidationError::MissingPathParam {
                    command: cmd.name.clone(),
                    placeholder: placeholder.clone(),
                });
            }
        }

        for param in &cmd.params {
            if let Some(crate::spec::ParamSource::Path) = param.source {
                if !path_placeholders.contains(&param.name) {
                    errors.push(ValidationError::OrphanPathParam {
                        command: cmd.name.clone(),
                        param: param.name.clone(),
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
