//! Persistent CLI configuration — base_url, output format, etc.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to read config: {0}")]
    ReadError(String),
    #[error("Failed to write config: {0}")]
    WriteError(String),
    #[error("Unknown config key: {0}")]
    UnknownKey(String),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CliConfig {
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub output_format: Option<String>,
    #[serde(default)]
    pub timeout: Option<u64>,
    #[serde(default)]
    pub pretty: Option<bool>,
}

impl CliConfig {
    pub fn load(path: &PathBuf) -> Self {
        if path.exists() {
            let content = std::fs::read_to_string(path).unwrap_or_default();
            toml::from_str(&content).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self, path: &PathBuf) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ConfigError::WriteError(e.to_string()))?;
        }
        let content = toml::to_string_pretty(self)
            .map_err(|e| ConfigError::WriteError(e.to_string()))?;
        std::fs::write(path, content)
            .map_err(|e| ConfigError::WriteError(e.to_string()))
    }

    pub fn set(&mut self, key: &str, value: &str) -> Result<(), ConfigError> {
        match key {
            "base_url" => self.base_url = Some(value.to_string()),
            "output_format" => self.output_format = Some(value.to_string()),
            "timeout" => self.timeout = Some(value.parse().map_err(|_| ConfigError::UnknownKey("timeout must be a number".to_string()))?),
            "pretty" => self.pretty = Some(value.parse().map_err(|_| ConfigError::UnknownKey("pretty must be true/false".to_string()))?),
            _ => return Err(ConfigError::UnknownKey(key.to_string())),
        }
        Ok(())
    }

    pub fn get(&self, key: &str) -> Result<String, ConfigError> {
        match key {
            "base_url" => Ok(self.base_url.clone().unwrap_or_else(|| "(not set)".to_string())),
            "output_format" => Ok(self.output_format.clone().unwrap_or_else(|| "(not set)".to_string())),
            "timeout" => Ok(self.timeout.map(|t| t.to_string()).unwrap_or_else(|| "(not set)".to_string())),
            "pretty" => Ok(self.pretty.map(|p| p.to_string()).unwrap_or_else(|| "(not set)".to_string())),
            _ => Err(ConfigError::UnknownKey(key.to_string())),
        }
    }

    pub fn list(&self) -> Vec<(String, String)> {
        vec![
            ("base_url".to_string(), self.base_url.clone().unwrap_or_else(|| "(not set)".to_string())),
            ("output_format".to_string(), self.output_format.clone().unwrap_or_else(|| "(not set)".to_string())),
            ("timeout".to_string(), self.timeout.map(|t| t.to_string()).unwrap_or_else(|| "(not set)".to_string())),
            ("pretty".to_string(), self.pretty.map(|p| p.to_string()).unwrap_or_else(|| "(not set)".to_string())),
        ]
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
