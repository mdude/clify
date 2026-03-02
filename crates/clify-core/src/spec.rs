//! Clify spec types — the Rust representation of a .clify.yaml file.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Root of a .clify.yaml spec file.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ClifySpec {
    pub meta: Meta,
    pub transport: Transport,
    pub auth: Auth,
    #[serde(default)]
    pub output: Output,
    #[serde(default)]
    pub config: Config,
    #[serde(default)]
    pub groups: Vec<Group>,
    pub commands: Vec<Command>,
    #[serde(default)]
    pub hooks: Option<GlobalHooks>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Meta {
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub long_description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Transport {
    #[serde(rename = "type")]
    pub transport_type: TransportType,
    pub base_url: String,
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    #[serde(default)]
    pub retries: u32,
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TransportType {
    Rest,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Auth {
    None,
    ApiKey {
        location: ApiKeyLocation,
        name: String,
        env: String,
    },
    Token {
        env: String,
    },
    Basic {
        env_user: String,
        env_pass: String,
    },
    Oauth2 {
        grant: OAuthGrant,
        token_url: String,
        #[serde(default)]
        authorize_url: Option<String>,
        #[serde(default)]
        scopes: Vec<String>,
        env_client_id: String,
        env_client_secret: String,
        #[serde(default)]
        custom: Option<OAuthCustom>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ApiKeyLocation {
    Header,
    Query,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OAuthGrant {
    ClientCredentials,
    AuthorizationCode,
    DeviceCode,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OAuthCustom {
    #[serde(default = "default_token_field")]
    pub token_field: String,
    #[serde(default = "default_expiry_field")]
    pub expiry_field: String,
    #[serde(default = "default_content_type")]
    pub content_type: ContentType,
    #[serde(default)]
    pub extra_params: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Output {
    #[serde(default = "default_format")]
    pub default_format: OutputFormat,
    #[serde(default = "default_true")]
    pub pretty: bool,
    #[serde(default)]
    pub table: TableConfig,
}

impl Default for Output {
    fn default() -> Self {
        Self {
            default_format: OutputFormat::Json,
            pretty: true,
            table: TableConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct TableConfig {
    #[serde(default)]
    pub max_width: Option<u32>,
    #[serde(default)]
    pub style: TableStyle,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TableStyle {
    #[default]
    Plain,
    Rounded,
    Sharp,
    Markdown,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    #[default]
    Json,
    Table,
    Csv,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Config {
    #[serde(default)]
    pub path: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self { path: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Group {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Command {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub long_description: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub hidden: bool,
    pub request: Request,
    #[serde(default)]
    pub params: Vec<Param>,
    #[serde(default)]
    pub response: Option<Response>,
    #[serde(default)]
    pub examples: Vec<Example>,
    #[serde(default)]
    pub hooks: Option<CommandHooks>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Request {
    pub method: HttpMethod,
    pub path: String,
    #[serde(default = "default_content_type")]
    pub content_type: ContentType,
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ContentType {
    #[default]
    Json,
    Form,
    Multipart,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Param {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: ParamType,
    #[serde(default)]
    pub required: bool,
    pub description: String,
    #[serde(default)]
    pub short: Option<String>,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    #[serde(default)]
    pub env: Option<String>,
    #[serde(default)]
    pub source: Option<ParamSource>,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub values: Vec<String>,
    #[serde(default)]
    pub separator: Option<String>,
    #[serde(default)]
    pub file_type: Option<FileType>,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub validation: Option<Validation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ParamType {
    String,
    Integer,
    Float,
    Boolean,
    Enum,
    Array,
    File,
    Object,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ParamSource {
    Path,
    Query,
    Body,
    Header,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum FileType {
    Path,
    Stdin,
    Both,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct Validation {
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub min_length: Option<usize>,
    #[serde(default)]
    pub max_length: Option<usize>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub custom: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Response {
    #[serde(default = "default_success_status")]
    pub success_status: Vec<u16>,
    #[serde(default)]
    pub success_path: Option<String>,
    #[serde(default)]
    pub error_path: Option<String>,
    #[serde(default)]
    pub pagination: Option<Pagination>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Pagination {
    #[serde(rename = "type")]
    pub pagination_type: PaginationType,
    pub param: String,
    #[serde(default)]
    pub page_size_param: Option<String>,
    #[serde(default)]
    pub default_page_size: Option<u32>,
    #[serde(default)]
    pub next_path: Option<String>,
    #[serde(default)]
    pub total_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PaginationType {
    Offset,
    Cursor,
    Link,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Example {
    pub description: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CommandHooks {
    #[serde(default)]
    pub before: Option<String>,
    #[serde(default)]
    pub after: Option<String>,
    #[serde(default)]
    pub on_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GlobalHooks {
    #[serde(default)]
    pub global: Option<GlobalHookDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GlobalHookDef {
    #[serde(default)]
    pub before: Option<String>,
    #[serde(default)]
    pub after: Option<String>,
}

// Default value helpers
fn default_timeout() -> u64 { 30 }
fn default_token_field() -> String { "access_token".to_string() }
fn default_expiry_field() -> String { "expires_in".to_string() }
fn default_content_type() -> ContentType { ContentType::Json }
fn default_format() -> OutputFormat { OutputFormat::Json }
fn default_true() -> bool { true }
fn default_success_status() -> Vec<u16> { vec![200] }
