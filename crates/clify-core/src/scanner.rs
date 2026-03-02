//! Scanner — auto-generate .clify.yaml from OpenAPI/Swagger specs.

use crate::spec::*;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScanError {
    #[error("Failed to read spec: {0}")]
    ReadError(String),
    #[error("Failed to parse spec: {0}")]
    ParseError(String),
    #[error("Failed to fetch URL: {0}")]
    FetchError(String),
    #[error("Unsupported spec format: {0}")]
    UnsupportedFormat(String),
}

pub struct Scanner;

impl Scanner {
    /// Scan an OpenAPI 3.x spec file and produce a ClifySpec.
    pub fn from_openapi(content: &str) -> Result<ClifySpec, ScanError> {
        let openapi: openapiv3::OpenAPI = serde_yaml::from_str(content)
            .or_else(|_| serde_json::from_str(content).map_err(|e| e.to_string()))
            .map_err(|e| ScanError::ParseError(format!("Not valid OpenAPI: {}", e)))?;

        let meta = extract_meta(&openapi);
        let transport = extract_transport(&openapi);
        let auth = extract_auth(&openapi);
        let (groups, commands) = extract_commands(&openapi);

        Ok(ClifySpec {
            meta,
            transport,
            auth,
            output: Output::default(),
            config: Config::default(),
            groups,
            commands,
            hooks: None,
        })
    }

    /// Scan a Swagger 2.0 spec — convert to OpenAPI 3.x internally.
    pub fn from_swagger(content: &str) -> Result<ClifySpec, ScanError> {
        // Swagger 2.0 has a different structure. We parse the key fields manually.
        let swagger: serde_json::Value = serde_yaml::from_str(content)
            .or_else(|_| serde_json::from_str(content).map_err(|e| e.to_string()))
            .map_err(|e| ScanError::ParseError(format!("Not valid Swagger: {}", e)))?;

        let version = swagger.get("swagger").and_then(|v| v.as_str()).unwrap_or("");
        if !version.starts_with("2.") {
            return Err(ScanError::UnsupportedFormat(
                format!("Expected Swagger 2.x, got: {}", version)
            ));
        }

        let title = swagger.pointer("/info/title")
            .and_then(|v| v.as_str())
            .unwrap_or("api");
        let version = swagger.pointer("/info/version")
            .and_then(|v| v.as_str())
            .unwrap_or("0.1.0");
        let description = swagger.pointer("/info/description")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let host = swagger.get("host").and_then(|v| v.as_str()).unwrap_or("localhost");
        let base_path = swagger.get("basePath").and_then(|v| v.as_str()).unwrap_or("/");
        let scheme = swagger.get("schemes")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_str())
            .unwrap_or("https");

        let base_url = format!("{}://{}{}", scheme, host, base_path);
        let cli_name = slugify(title);

        let meta = Meta {
            name: cli_name,
            version: clean_version(version),
            description: description.to_string(),
            long_description: None,
            author: None,
            license: None,
            homepage: None,
        };

        let transport = Transport {
            transport_type: TransportType::Rest,
            base_url,
            timeout: 30,
            retries: 0,
            headers: HashMap::new(),
        };

        // Extract auth from securityDefinitions
        let auth = extract_swagger_auth(&swagger);

        // Extract commands from paths
        let (groups, commands) = extract_swagger_commands(&swagger);

        Ok(ClifySpec {
            meta,
            transport,
            auth,
            output: Output::default(),
            config: Config::default(),
            groups,
            commands,
            hooks: None,
        })
    }

    /// Fetch a spec from a URL and auto-detect format.
    pub async fn from_url(url: &str) -> Result<(String, String), ScanError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| ScanError::FetchError(e.to_string()))?;

        // Try the URL directly first
        let resp = client.get(url).send().await
            .map_err(|e| ScanError::FetchError(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ScanError::FetchError(format!("HTTP {}", resp.status())));
        }

        let content = resp.text().await
            .map_err(|e| ScanError::FetchError(e.to_string()))?;

        // Detect format
        let format = if content.contains("\"openapi\"") || content.contains("openapi:") {
            "openapi"
        } else if content.contains("\"swagger\"") || content.contains("swagger:") {
            "swagger"
        } else {
            return Err(ScanError::UnsupportedFormat("Could not detect spec format".to_string()));
        };

        Ok((content, format.to_string()))
    }

    /// Serialize a ClifySpec to YAML.
    pub fn to_yaml(spec: &ClifySpec) -> Result<String, ScanError> {
        serde_yaml::to_string(spec)
            .map_err(|e| ScanError::ParseError(e.to_string()))
    }
}

// --- OpenAPI 3.x extraction ---

fn extract_meta(api: &openapiv3::OpenAPI) -> Meta {
    let name = slugify(&api.info.title);
    Meta {
        name,
        version: clean_version(&api.info.version),
        description: api.info.description.clone().unwrap_or_else(|| api.info.title.clone()),
        long_description: None,
        author: api.info.contact.as_ref().and_then(|c| c.name.clone()),
        license: api.info.license.as_ref().map(|l| l.name.clone()),
        homepage: api.info.contact.as_ref().and_then(|c| c.url.clone()),
    }
}

fn extract_transport(api: &openapiv3::OpenAPI) -> Transport {
    let raw_url = api.servers.first()
        .map(|s| s.url.clone())
        .unwrap_or_else(|| "https://api.example.com".to_string());

    // Ensure base_url is absolute
    let base_url = if raw_url.starts_with("http://") || raw_url.starts_with("https://") {
        raw_url
    } else {
        format!("https://api.example.com{}", raw_url)
    };

    Transport {
        transport_type: TransportType::Rest,
        base_url,
        timeout: 30,
        retries: 0,
        headers: HashMap::from([
            ("Accept".to_string(), "application/json".to_string()),
        ]),
    }
}

fn extract_auth(api: &openapiv3::OpenAPI) -> Auth {
    if let Some(components) = &api.components {
        for (_name, scheme_ref) in &components.security_schemes {
            if let openapiv3::ReferenceOr::Item(scheme) = scheme_ref {
                match scheme {
                    openapiv3::SecurityScheme::APIKey { location, name, .. } => {
                        let loc = match location {
                            openapiv3::APIKeyLocation::Header => ApiKeyLocation::Header,
                            openapiv3::APIKeyLocation::Query => ApiKeyLocation::Query,
                            _ => ApiKeyLocation::Header,
                        };
                        return Auth::ApiKey {
                            location: loc,
                            name: name.clone(),
                            env: "API_KEY".to_string(),
                        };
                    }
                    openapiv3::SecurityScheme::HTTP { scheme, .. } => {
                        if scheme.to_lowercase() == "bearer" {
                            return Auth::Token { env: "API_TOKEN".to_string() };
                        } else if scheme.to_lowercase() == "basic" {
                            return Auth::Basic {
                                env_user: "API_USER".to_string(),
                                env_pass: "API_PASS".to_string(),
                            };
                        }
                    }
                    openapiv3::SecurityScheme::OAuth2 { flows, .. } => {
                        if let Some(cc) = &flows.client_credentials {
                            return Auth::Oauth2 {
                                grant: OAuthGrant::ClientCredentials,
                                token_url: cc.token_url.clone(),
                                authorize_url: None,
                                scopes: cc.scopes.keys().cloned().collect(),
                                env_client_id: "OAUTH_CLIENT_ID".to_string(),
                                env_client_secret: "OAUTH_CLIENT_SECRET".to_string(),
                                custom: None,
                            };
                        }
                        if let Some(ac) = &flows.authorization_code {
                            return Auth::Oauth2 {
                                grant: OAuthGrant::AuthorizationCode,
                                token_url: ac.token_url.clone(),
                                authorize_url: Some(ac.authorization_url.clone()),
                                scopes: ac.scopes.keys().cloned().collect(),
                                env_client_id: "OAUTH_CLIENT_ID".to_string(),
                                env_client_secret: "OAUTH_CLIENT_SECRET".to_string(),
                                custom: None,
                            };
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    Auth::None
}

fn extract_commands(api: &openapiv3::OpenAPI) -> (Vec<Group>, Vec<Command>) {
    let mut groups_map: HashMap<String, Group> = HashMap::new();
    let mut commands = Vec::new();

    for (path, path_item_ref) in &api.paths.paths {
        let path_item = match path_item_ref {
            openapiv3::ReferenceOr::Item(item) => item,
            _ => continue,
        };

        let operations = [
            ("GET", &path_item.get),
            ("POST", &path_item.post),
            ("PUT", &path_item.put),
            ("PATCH", &path_item.patch),
            ("DELETE", &path_item.delete),
        ];

        for (method_str, op_opt) in &operations {
            if let Some(op) = op_opt {
                let (group_name, cmd_name) = path_to_command_name(path, method_str);

                // Create group if needed
                if let Some(ref gn) = group_name {
                    if !groups_map.contains_key(gn) {
                        let desc = op.tags.first()
                            .cloned()
                            .unwrap_or_else(|| format!("{} operations", gn));
                        groups_map.insert(gn.clone(), Group {
                            name: gn.clone(),
                            description: desc,
                        });
                    }
                }

                let description = op.summary.clone()
                    .or_else(|| op.description.clone())
                    .unwrap_or_else(|| format!("{} {}", method_str, path));

                let method = match *method_str {
                    "GET" => HttpMethod::Get,
                    "POST" => HttpMethod::Post,
                    "PUT" => HttpMethod::Put,
                    "PATCH" => HttpMethod::Patch,
                    "DELETE" => HttpMethod::Delete,
                    _ => HttpMethod::Get,
                };

                // Extract parameters
                let params = extract_operation_params(op, &path_item.parameters, method_str);

                // Normalize path placeholders to match slugified param names
                // e.g., {petId} → {pet-id} to match the param name after slugify
                let request_path = normalize_path_placeholders(path, &params);

                // Extract response handling
                let response = extract_response(op);

                let cmd = Command {
                    name: cmd_name,
                    description,
                    long_description: op.description.clone(),
                    group: group_name,
                    aliases: vec![],
                    hidden: op.deprecated,
                    request: Request {
                        method,
                        path: request_path,
                        content_type: extract_content_type(op),
                        headers: HashMap::new(),
                    },
                    params,
                    response: Some(response),
                    examples: vec![],
                    hooks: None,
                };

                commands.push(cmd);
            }
        }
    }

    // Deduplicate command names within the same group
    let mut seen: HashMap<(Option<String>, String), usize> = HashMap::new();
    for cmd in &mut commands {
        let key = (cmd.group.clone(), cmd.name.clone());
        let count = seen.entry(key).or_insert(0);
        *count += 1;
        if *count > 1 {
            cmd.name = format!("{}-{}", cmd.name, count);
        }
    }

    let groups: Vec<Group> = groups_map.into_values().collect();
    (groups, commands)
}

fn extract_operation_params(
    op: &openapiv3::Operation,
    path_params: &[openapiv3::ReferenceOr<openapiv3::Parameter>],
    _method: &str,
) -> Vec<Param> {
    let mut params = Vec::new();

    // Path-level parameters
    for param_ref in path_params {
        if let openapiv3::ReferenceOr::Item(param) = param_ref {
            if let Some(p) = convert_parameter(param) {
                params.push(p);
            }
        }
    }

    // Operation-level parameters
    for param_ref in &op.parameters {
        if let openapiv3::ReferenceOr::Item(param) = param_ref {
            if let Some(p) = convert_parameter(param) {
                // Don't duplicate
                if !params.iter().any(|existing| existing.name == p.name) {
                    params.push(p);
                }
            }
        }
    }

    // Request body parameters
    if let Some(openapiv3::ReferenceOr::Item(body)) = &op.request_body {
        if let Some(media) = body.content.get("application/json") {
            if let Some(openapiv3::ReferenceOr::Item(schema)) = &media.schema {
                extract_schema_params(schema, &mut params, ParamSource::Body);
            }
        }
    }

    params
}

fn convert_parameter(param: &openapiv3::Parameter) -> Option<Param> {
    let (name, data, source) = match param {
        openapiv3::Parameter::Path { parameter_data, .. } => {
            (parameter_data.name.clone(), parameter_data, ParamSource::Path)
        }
        openapiv3::Parameter::Query { parameter_data, .. } => {
            (parameter_data.name.clone(), parameter_data, ParamSource::Query)
        }
        openapiv3::Parameter::Header { parameter_data, .. } => {
            (parameter_data.name.clone(), parameter_data, ParamSource::Header)
        }
        _ => return None,
    };

    let (param_type, values) = match &data.format {
        openapiv3::ParameterSchemaOrContent::Schema(schema_ref) => {
            match schema_ref {
                openapiv3::ReferenceOr::Item(schema) => schema_to_type(schema),
                _ => (ParamType::String, vec![]),
            }
        }
        _ => (ParamType::String, vec![]),
    };

    Some(Param {
        name: slugify_param(&name),
        param_type: if !values.is_empty() { ParamType::Enum } else { param_type },
        required: data.required,
        description: data.description.clone().unwrap_or_else(|| name.clone()),
        short: None,
        default: None,
        env: None,
        source: Some(source),
        hidden: data.deprecated.unwrap_or(false),
        values,
        separator: None,
        file_type: None,
        mime_type: None,
        validation: None,
    })
}

fn schema_to_type(schema: &openapiv3::Schema) -> (ParamType, Vec<String>) {
    match &schema.schema_kind {
        openapiv3::SchemaKind::Type(t) => match t {
            openapiv3::Type::String(s) => {
                if !s.enumeration.is_empty() {
                    let values: Vec<String> = s.enumeration.iter()
                        .filter_map(|v| v.clone())
                        .collect();
                    (ParamType::Enum, values)
                } else {
                    (ParamType::String, vec![])
                }
            }
            openapiv3::Type::Integer(_) => (ParamType::Integer, vec![]),
            openapiv3::Type::Number(_) => (ParamType::Float, vec![]),
            openapiv3::Type::Boolean(_) => (ParamType::Boolean, vec![]),
            openapiv3::Type::Array(_) => (ParamType::Array, vec![]),
            openapiv3::Type::Object(_) => (ParamType::Object, vec![]),
        },
        _ => (ParamType::String, vec![]),
    }
}

fn extract_schema_params(schema: &openapiv3::Schema, params: &mut Vec<Param>, source: ParamSource) {
    if let openapiv3::SchemaKind::Type(openapiv3::Type::Object(obj)) = &schema.schema_kind {
        let required_fields: std::collections::HashSet<_> = obj.required.iter().collect();
        for (name, prop_ref) in &obj.properties {
            if let openapiv3::ReferenceOr::Item(prop_schema) = prop_ref {
                let (param_type, values) = schema_to_type(prop_schema);
                let description = match &prop_schema.schema_data.description {
                    Some(d) => d.clone(),
                    None => name.clone(),
                };
                params.push(Param {
                    name: slugify_param(name),
                    param_type: if !values.is_empty() { ParamType::Enum } else { param_type },
                    required: required_fields.contains(name),
                    description,
                    short: None,
                    default: None,
                    env: None,
                    source: Some(source.clone()),
                    hidden: false,
                    values,
                    separator: None,
                    file_type: None,
                    mime_type: None,
                    validation: None,
                });
            }
        }
    }
}

fn extract_content_type(op: &openapiv3::Operation) -> ContentType {
    if let Some(openapiv3::ReferenceOr::Item(body)) = &op.request_body {
        if body.content.contains_key("application/x-www-form-urlencoded") {
            return ContentType::Form;
        }
        if body.content.contains_key("multipart/form-data") {
            return ContentType::Multipart;
        }
    }
    ContentType::Json
}

fn extract_response(op: &openapiv3::Operation) -> Response {
    let success_status: Vec<u16> = op.responses.responses.keys()
        .filter_map(|k| match k {
            openapiv3::StatusCode::Code(c) if *c >= 200 && *c < 300 => Some(*c),
            _ => None,
        })
        .collect();

    Response {
        success_status: if success_status.is_empty() { vec![200] } else { success_status },
        success_path: None,
        error_path: None,
        pagination: None,
    }
}

// --- Swagger 2.0 extraction ---

fn extract_swagger_auth(swagger: &serde_json::Value) -> Auth {
    if let Some(defs) = swagger.get("securityDefinitions").and_then(|v| v.as_object()) {
        for (_name, def) in defs {
            let auth_type = def.get("type").and_then(|v| v.as_str()).unwrap_or("");
            match auth_type {
                "apiKey" => {
                    let location = match def.get("in").and_then(|v| v.as_str()).unwrap_or("header") {
                        "query" => ApiKeyLocation::Query,
                        _ => ApiKeyLocation::Header,
                    };
                    let name = def.get("name").and_then(|v| v.as_str()).unwrap_or("Authorization").to_string();
                    return Auth::ApiKey { location, name, env: "API_KEY".to_string() };
                }
                "basic" => {
                    return Auth::Basic {
                        env_user: "API_USER".to_string(),
                        env_pass: "API_PASS".to_string(),
                    };
                }
                "oauth2" => {
                    let token_url = def.get("tokenUrl").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    return Auth::Oauth2 {
                        grant: OAuthGrant::ClientCredentials,
                        token_url,
                        authorize_url: def.get("authorizationUrl").and_then(|v| v.as_str()).map(|s| s.to_string()),
                        scopes: vec![],
                        env_client_id: "OAUTH_CLIENT_ID".to_string(),
                        env_client_secret: "OAUTH_CLIENT_SECRET".to_string(),
                        custom: None,
                    };
                }
                _ => {}
            }
        }
    }
    Auth::None
}

fn extract_swagger_commands(swagger: &serde_json::Value) -> (Vec<Group>, Vec<Command>) {
    let mut groups_map: HashMap<String, Group> = HashMap::new();
    let mut commands = Vec::new();

    let paths = match swagger.get("paths").and_then(|v| v.as_object()) {
        Some(p) => p,
        None => return (vec![], vec![]),
    };

    for (path, methods) in paths {
        let methods_obj = match methods.as_object() {
            Some(m) => m,
            None => continue,
        };

        for (method, op) in methods_obj {
            if !["get", "post", "put", "patch", "delete"].contains(&method.as_str()) {
                continue;
            }

            let (group_name, cmd_name) = path_to_command_name(path, &method.to_uppercase());

            if let Some(ref gn) = group_name {
                if !groups_map.contains_key(gn) {
                    let desc = op.get("tags")
                        .and_then(|v| v.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|v| v.as_str())
                        .unwrap_or(gn)
                        .to_string();
                    groups_map.insert(gn.clone(), Group { name: gn.clone(), description: desc });
                }
            }

            let description = op.get("summary")
                .or_else(|| op.get("description"))
                .and_then(|v| v.as_str())
                .unwrap_or(&format!("{} {}", method.to_uppercase(), path))
                .to_string();

            let http_method = match method.as_str() {
                "get" => HttpMethod::Get,
                "post" => HttpMethod::Post,
                "put" => HttpMethod::Put,
                "patch" => HttpMethod::Patch,
                "delete" => HttpMethod::Delete,
                _ => HttpMethod::Get,
            };

            // Extract params
            let mut params = Vec::new();
            if let Some(param_array) = op.get("parameters").and_then(|v| v.as_array()) {
                for p in param_array {
                    let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let in_field = p.get("in").and_then(|v| v.as_str()).unwrap_or("query");
                    let required = p.get("required").and_then(|v| v.as_bool()).unwrap_or(false);
                    let desc = p.get("description").and_then(|v| v.as_str()).unwrap_or(&name).to_string();
                    let p_type = p.get("type").and_then(|v| v.as_str()).unwrap_or("string");

                    let source = match in_field {
                        "path" => ParamSource::Path,
                        "query" => ParamSource::Query,
                        "header" => ParamSource::Header,
                        "body" => ParamSource::Body,
                        _ => ParamSource::Query,
                    };

                    let param_type = match p_type {
                        "integer" => ParamType::Integer,
                        "number" => ParamType::Float,
                        "boolean" => ParamType::Boolean,
                        "array" => ParamType::Array,
                        _ => ParamType::String,
                    };

                    let values: Vec<String> = p.get("enum")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                        .unwrap_or_default();

                    if name.is_empty() { continue; }

                    params.push(Param {
                        name: slugify_param(&name),
                        param_type: if !values.is_empty() { ParamType::Enum } else { param_type },
                        required,
                        description: desc,
                        short: None,
                        default: None,
                        env: None,
                        source: Some(source),
                        hidden: false,
                        values,
                        separator: None,
                        file_type: None,
                        mime_type: None,
                        validation: None,
                    });
                }
            }

            let cmd = Command {
                name: cmd_name,
                description,
                long_description: op.get("description").and_then(|v| v.as_str()).map(|s| s.to_string()),
                group: group_name,
                aliases: vec![],
                hidden: op.get("deprecated").and_then(|v| v.as_bool()).unwrap_or(false),
                request: Request {
                    method: http_method,
                    path: path.clone(),
                    content_type: ContentType::Json,
                    headers: HashMap::new(),
                },
                params,
                response: Some(Response {
                    success_status: vec![200],
                    success_path: None,
                    error_path: None,
                    pagination: None,
                }),
                examples: vec![],
                hooks: None,
            };
            commands.push(cmd);
        }
    }

    let groups: Vec<Group> = groups_map.into_values().collect();
    (groups, commands)
}

// --- Helpers ---

/// Normalize path placeholders to match slugified param names.
/// {petId} → {pet-id}, {orderId} → {order-id}
fn normalize_path_placeholders(path: &str, params: &[Param]) -> String {
    let mut result = path.to_string();
    let re = regex::Regex::new(r"\{(\w+)\}").unwrap();
    for cap in re.captures_iter(path) {
        let original = &cap[1];
        let slugified = slugify_param(original);
        // Only replace if we have a matching param with this slugified name
        if params.iter().any(|p| p.name == slugified && matches!(p.source, Some(ParamSource::Path))) {
            result = result.replace(&format!("{{{}}}", original), &format!("{{{}}}", slugified));
        }
    }
    result
}

/// Convert an API path + method to (group_name, command_name).
/// /api/v1/users/{id}/posts  GET  → (Some("users"), "list-posts")
/// /api/v1/users             POST → (Some("users"), "create-user")
/// /pets                     GET  → (None, "list-pets")
fn path_to_command_name(path: &str, method: &str) -> (Option<String>, String) {
    let segments: Vec<&str> = path.split('/')
        .filter(|s| !s.is_empty() && !s.starts_with('{') && !["api", "v1", "v2", "v3", "rest"].contains(s))
        .collect();

    if segments.is_empty() {
        return (None, format!("{}-root", method.to_lowercase()));
    }

    let group = if segments.len() > 1 {
        Some(slugify(segments[0]))
    } else {
        None
    };

    let resource = segments.last().unwrap();
    let cmd_name = match method {
        "GET" => {
            if path.ends_with('}') {
                format!("get-{}", singularize(resource))
            } else {
                format!("list-{}", slugify(resource))
            }
        }
        "POST" => format!("create-{}", singularize(resource)),
        "PUT" | "PATCH" => format!("update-{}", singularize(resource)),
        "DELETE" => format!("delete-{}", singularize(resource)),
        _ => format!("{}-{}", method.to_lowercase(), slugify(resource)),
    };

    (group, cmd_name)
}

/// Simple singularize (just remove trailing 's').
fn singularize(s: &str) -> String {
    let slug = slugify(s);
    if slug.ends_with('s') && slug.len() > 1 {
        slug[..slug.len() - 1].to_string()
    } else {
        slug
    }
}

/// Convert a string to a CLI-friendly slug.
fn slugify(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Convert a param name to CLI style (camelCase → kebab-case).
fn slugify_param(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push('-');
        }
        result.push(c.to_lowercase().next().unwrap_or(c));
    }
    result.replace('_', "-").replace(' ', "-")
}

/// Clean a version string to be semver-compatible.
fn clean_version(v: &str) -> String {
    let v = v.trim_start_matches('v');
    let parts: Vec<&str> = v.split('.').collect();
    match parts.len() {
        1 => format!("{}.0.0", parts[0]),
        2 => format!("{}.{}.0", parts[0], parts[1]),
        _ => parts[..3].join("."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_to_command_name() {
        assert_eq!(path_to_command_name("/users", "GET"), (None, "list-users".to_string()));
        assert_eq!(path_to_command_name("/users/{id}", "GET"), (None, "get-user".to_string()));
        assert_eq!(path_to_command_name("/users", "POST"), (None, "create-user".to_string()));
        assert_eq!(path_to_command_name("/users/{id}", "DELETE"), (None, "delete-user".to_string()));
        assert_eq!(path_to_command_name("/api/v1/users/{id}/posts", "GET").1, "list-posts".to_string());
    }

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("ArcGIS Server"), "arcgis-server");
        assert_eq!(slugify("my_api v2"), "my-api-v2");
    }

    #[test]
    fn test_slugify_param() {
        assert_eq!(slugify_param("resultOffset"), "result-offset");
        assert_eq!(slugify_param("page_size"), "page-size");
    }

    #[test]
    fn test_clean_version() {
        assert_eq!(clean_version("1"), "1.0.0");
        assert_eq!(clean_version("1.0"), "1.0.0");
        assert_eq!(clean_version("v1.2.3"), "1.2.3");
    }

    #[test]
    fn test_scan_petstore_openapi() {
        let yaml = r#"
openapi: "3.0.0"
info:
  title: "Petstore"
  version: "1.0.0"
  description: "A sample pet store API"
servers:
  - url: https://petstore.example.com/api
paths:
  /pets:
    get:
      summary: "List all pets"
      operationId: listPets
      parameters:
        - name: limit
          in: query
          required: false
          schema:
            type: integer
      responses:
        '200':
          description: A list of pets
    post:
      summary: "Create a pet"
      operationId: createPet
      requestBody:
        content:
          application/json:
            schema:
              type: object
              required: [name]
              properties:
                name:
                  type: string
                  description: "Pet name"
                tag:
                  type: string
                  description: "Pet tag"
      responses:
        '201':
          description: Null response
  /pets/{petId}:
    get:
      summary: "Get a pet"
      operationId: showPetById
      parameters:
        - name: petId
          in: path
          required: true
          schema:
            type: string
      responses:
        '200':
          description: A pet
"#;
        let spec = Scanner::from_openapi(yaml).expect("Failed to scan");
        assert_eq!(spec.meta.name, "petstore");
        assert_eq!(spec.meta.version, "1.0.0");
        assert_eq!(spec.transport.base_url, "https://petstore.example.com/api");
        assert!(spec.commands.len() >= 3);
        // Check we got the right commands
        let cmd_names: Vec<&str> = spec.commands.iter().map(|c| c.name.as_str()).collect();
        assert!(cmd_names.contains(&"list-pets"));
        assert!(cmd_names.contains(&"create-pet"));
        assert!(cmd_names.contains(&"get-pet"));
    }
}
